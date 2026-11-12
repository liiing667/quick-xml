//! Limits for recursive expansion of custom (general) entities.
//!
//! Custom entities declared in a DTD (`<!ENTITY name "replacement text">`) can
//! reference other entities in their replacement text. When XML comes from an
//! untrusted source, a small document can therefore expand into a huge amount
//! of data ("billion laughs" / entity expansion attacks) or recurse forever
//! through an entity cycle.
//!
//! [`EntityExpansion`] is a composable configuration that bounds the expansion
//! in three independent ways and detects cycles:
//!
//! - [nesting depth](EntityExpansion::max_depth) limits how deeply entities can
//!   be nested in their replacement texts;
//! - [total output bytes](EntityExpansion::max_bytes) limits how much memory the
//!   expanded content can consume;
//! - [number of references](EntityExpansion::max_references) limits how many
//!   entity references are expanded in total;
//! - a reference to an entity that is already being expanded is reported as a
//!   [cycle](crate::escape::EscapeError::EntityCycleDetected).
//!
//! The same configuration is used by text unescaping, attribute normalization
//! and deserialization, so all expansion paths share one budget per expansion
//! operation. Use [`EntityExpansion::UNLIMITED`] to disable all limits.

use std::borrow::Cow;
use std::collections::HashSet;
use std::hash::{BuildHasherDefault, DefaultHasher};

use super::{EscapeError, parse_number};

/// Default maximum nesting depth of custom entities (128 levels).
pub const DEFAULT_ENTITY_DEPTH: usize = 128;
/// Default maximum number of expanded custom entity references (10 000).
pub const DEFAULT_ENTITY_REFERENCES: usize = 10_000;
/// Default maximum size of the expanded content in bytes (10 MiB).
pub const DEFAULT_ENTITY_BYTES: usize = 10 * 1024 * 1024;

type EntitySet = HashSet<String, BuildHasherDefault<DefaultHasher>>;

/// Composable limits for recursive expansion of custom entities.
///
/// All limits are optional and can be combined freely. A limit set to `None`
/// is not enforced. The [`Default`] implementation provides conservative limits
/// suitable for parsing untrusted XML; [`EntityExpansion::UNLIMITED`] disables
/// all limits and restores the unbounded behavior.
///
/// # Example
///
/// ```
/// use quick_xml::escape::EntityExpansion;
///
/// // Allow at most 8 nesting levels and 1 KiB of expanded content,
/// // but do not limit the number of references
/// let expansion = EntityExpansion::default()
///     .with_max_depth(8)
///     .with_max_bytes(1024)
///     .with_max_references(None);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntityExpansion {
    /// Maximum number of nested custom entity expansions.
    pub max_depth: Option<usize>,
    /// Maximum number of bytes the expanded content may contain.
    pub max_bytes: Option<usize>,
    /// Maximum number of custom entity references that may be expanded.
    pub max_references: Option<usize>,
}

impl Default for EntityExpansion {
    /// Conservative limits suitable for untrusted input:
    ///
    /// - [`DEFAULT_ENTITY_DEPTH`] nesting levels,
    /// - [`DEFAULT_ENTITY_BYTES`] bytes of expanded content,
    /// - [`DEFAULT_ENTITY_REFERENCES`] expanded references.
    fn default() -> Self {
        Self {
            max_depth: Some(DEFAULT_ENTITY_DEPTH),
            max_bytes: Some(DEFAULT_ENTITY_BYTES),
            max_references: Some(DEFAULT_ENTITY_REFERENCES),
        }
    }
}

impl EntityExpansion {
    /// Configuration that does not enforce any limit. Note that without limits
    /// a malicious document can consume unbounded memory or recurse forever.
    pub const UNLIMITED: Self = Self {
        max_depth: None,
        max_bytes: None,
        max_references: None,
    };

    /// Builder-style setter for the [nesting depth limit](Self::max_depth).
    #[must_use]
    pub const fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = Some(max_depth);
        self
    }

    /// Builder-style setter for the [output byte limit](Self::max_bytes).
    #[must_use]
    pub const fn with_max_bytes(mut self, max_bytes: usize) -> Self {
        self.max_bytes = Some(max_bytes);
        self
    }

    /// Builder-style setter for the [reference count limit](Self::max_references).
    /// Pass `None` to disable this particular limit.
    #[must_use]
    pub const fn with_max_references(mut self, max_references: Option<usize>) -> Self {
        self.max_references = max_references;
        self
    }

    /// Starts a new expansion run with these limits.
    pub(crate) fn guard(self) -> ExpansionGuard {
        ExpansionGuard {
            config: self,
            depth: 0,
            bytes: 0,
            references: 0,
            active: EntitySet::default(),
        }
    }
}

/// Runtime state shared by all entity expansions performed for one value.
///
/// One guard is created per expansion operation (one unescape call, one
/// attribute normalization or one deserialized text fragment), so the byte and
/// reference budgets cannot be exhausted piecewise by nested expansions.
pub(crate) struct ExpansionGuard {
    config: EntityExpansion,
    /// Current nesting depth of custom entity expansions (root level is zero).
    depth: usize,
    /// Number of bytes already emitted to the output.
    bytes: usize,
    /// Number of custom entity references already expanded.
    references: usize,
    /// Names of the entities that are currently being expanded on the stack.
    active: EntitySet,
}

impl ExpansionGuard {
    /// Reserves `count` bytes of output space, returning an error if the
    /// configured byte limit would be exceeded.
    #[inline]
    pub(crate) fn reserve_bytes(&mut self, count: usize) -> Result<(), EscapeError> {
        self.bytes = self.bytes.checked_add(count).ok_or_else(|| {
            EscapeError::EntityByteLimitExceeded(self.config.max_bytes.unwrap_or(usize::MAX))
        })?;
        if let Some(limit) = self.config.max_bytes {
            if self.bytes > limit {
                return Err(EscapeError::EntityByteLimitExceeded(limit));
            }
        }
        Ok(())
    }

    /// Checks that another custom entity with the given `name` may be expanded,
    /// updating the depth, reference and cycle budgets.
    pub(crate) fn enter_entity(&mut self, name: &str) -> Result<(), EscapeError> {
        let next_depth = self.depth.saturating_add(1);
        if let Some(limit) = self.config.max_depth {
            if next_depth > limit {
                return Err(EscapeError::EntityDepthLimitExceeded(next_depth));
            }
        }
        let next_references = self.references.saturating_add(1);
        if let Some(limit) = self.config.max_references {
            if next_references > limit {
                return Err(EscapeError::EntityReferenceLimitExceeded(limit));
            }
        }
        // Only mutate the state once all limits are satisfied, so a rejected
        // expansion never leaves a dangling entry in the cycle set.
        if !self.active.insert(name.to_owned()) {
            return Err(EscapeError::EntityCycleDetected(name.to_owned()));
        }
        self.depth = next_depth;
        self.references = next_references;
        Ok(())
    }

    /// Restores the depth and cycle budgets after an entity expansion finishes.
    ///
    /// Bytes and references are deliberately not restored: they measure the
    /// total work of the whole expansion operation.
    #[inline]
    pub(crate) fn leave_entity(&mut self, name: &str) {
        self.active.remove(name);
        self.depth = self.depth.saturating_sub(1);
    }
}

/// Expands character and custom entity references in a textual value.
///
/// Unlike [`unescape_with`], references found in replacement texts are expanded
/// recursively, and all expansions are bounded by the shared `guard`.
///
/// Literal content (everything that is not a reference) is copied verbatim and
/// counted against the byte budget. Predefined entities are resolved exactly as
/// in [`unescape_with`]: `resolve_entity` is tried first and the predefined
/// resolver is used as a fallback, so custom resolvers can override predefined
/// entities and unknown entities still produce
/// [`EscapeError::UnrecognizedEntity`].
pub(crate) fn expand_text<'input, 'entity, F>(
    raw: &'input str,
    guard: &mut ExpansionGuard,
    mut resolve_entity: F,
) -> Result<Cow<'input, str>, EscapeError>
where
    F: FnMut(&str) -> Option<&'entity str>,
{
    // Fast path: nothing that can be expanded, and the literal content fits.
    if memchr::memchr(b'&', raw.as_bytes()).is_none() {
        guard.reserve_bytes(raw.len())?;
        return Ok(Cow::Borrowed(raw));
    }

    // The input contains at least one `&` (see the fast path above), so an
    // output buffer is always required. It is allocated up front so literal
    // prefixes of nested entity frames cannot be lost.
    let mut output = String::with_capacity(raw.len());
    // Each frame is a (slice, next byte position) pair waiting to be resumed.
    // Frames for custom entities also carry the entity name so its depth and
    // cycle budget can be released when the expansion finishes. An explicit
    // work stack keeps deeply nested entities from growing the native stack.
    struct Frame<'a> {
        value: &'a str,
        pos: usize,
        entity: Option<&'a str>,
    }
    let mut stack: Vec<Frame> = vec![Frame {
        value: raw,
        pos: 0,
        entity: None,
    }];

    while let Some(frame) = stack.last_mut() {
        let value = frame.value;
        let bytes = value.as_bytes();
        let Some(amp) = memchr::memchr(b'&', &bytes[frame.pos..]).map(|i| frame.pos + i) else {
            // No more references in this frame: copy its remaining literal text
            // and pop it, restoring the parent frame (if any).
            let rest = &value[frame.pos..];
            guard.reserve_bytes(rest.len())?;
            output.push_str(rest);
            let finished = stack.pop().expect("frame exists");
            if let Some(name) = finished.entity {
                guard.leave_entity(name);
            }
            continue;
        };

        // Literal content before the reference.
        let literal = &value[frame.pos..amp];
        guard.reserve_bytes(literal.len())?;
        output.push_str(literal);

        let Some(semi) = memchr::memchr(b';', &bytes[amp + 1..]).map(|i| amp + 1 + i) else {
            return Err(EscapeError::UnterminatedEntity(amp..value.len()));
        };
        let pat = &value[amp + 1..semi];

        if let Some(num) = pat.strip_prefix('#') {
            // Character references are not entities: they do not count towards
            // depth, reference or cycle limits, but their output is byte-limited.
            let codepoint = parse_number(num).map_err(EscapeError::InvalidCharRef)?;
            let mut buf = [0u8; 4];
            let text = codepoint.encode_utf8(&mut buf);
            guard.reserve_bytes(text.len())?;
            output.push_str(text);
            frame.pos = semi + 1;
        } else if pat == "amp" {
            // `&amp;` resolves to a literal `&` that must not be interpreted as
            // the start of another reference, even when a custom resolver
            // overrides it. Handle it before the generic resolver dispatch.
            let replacement = resolve_entity("amp")
                .or_else(|| super::resolve_predefined_entity("amp"))
                .expect("`amp` is always a predefined entity");
            guard.reserve_bytes(replacement.len())?;
            output.push_str(replacement);
            frame.pos = semi + 1;
        } else {
            match resolve_entity(pat) {
                // A custom entity is always tracked as an expansion, even if its
                // replacement text happens to equal a predefined entity value.
                Some(replacement) => {
                    guard.enter_entity(pat)?;
                    guard.reserve_bytes(replacement.len())?;
                    // Resume this frame after the replacement frame is exhausted.
                    frame.pos = semi + 1;
                    stack.push(Frame {
                        value: replacement,
                        pos: 0,
                        entity: Some(pat),
                    });
                }
                // Other predefined entities expand to fixed, entity-free text
                // and must stay compatible with the old non-recursive behavior,
                // so they are not tracked as custom expansions.
                None => {
                    let replacement = super::resolve_predefined_entity(pat).ok_or_else(|| {
                        EscapeError::UnrecognizedEntity(amp + 1..semi, pat.to_owned())
                    })?;
                    guard.reserve_bytes(replacement.len())?;
                    output.push_str(replacement);
                    frame.pos = semi + 1;
                }
            }
        }
    }

    Ok(Cow::Owned(output))
}

#[cfg(feature = "serialize")]
/// Expands a single custom entity reference recursively, writing the result into
/// `result`. Used by the deserialization path, where the surrounding text is
/// streamed by the reader and is not passed as one string.
pub(crate) fn expand_entity_into<'entity, R>(
    result: &mut String,
    name: &str,
    guard: &mut ExpansionGuard,
    resolve_entity: &mut R,
) -> Result<(), EscapeError>
where
    R: FnMut(&str) -> Option<&'entity str>,
{
    // `&amp;` expands to a literal `&` that must never be reinterpreted as the
    // start of another reference, so it is emitted without descending.
    if name == "amp" {
        let replacement = resolve_entity("amp")
            .or_else(|| super::resolve_predefined_entity("amp"))
            .expect("`amp` is always a predefined entity");
        guard.reserve_bytes(replacement.len())?;
        result.push_str(replacement);
        return Ok(());
    }
    guard.enter_entity(name)?;
    let outcome = expand_entity_frame(result, name, guard, resolve_entity);
    guard.leave_entity(name);
    outcome
}

#[cfg(feature = "serialize")]
fn expand_entity_frame<'entity, R>(
    result: &mut String,
    name: &str,
    guard: &mut ExpansionGuard,
    resolve_entity: &mut R,
) -> Result<(), EscapeError>
where
    R: FnMut(&str) -> Option<&'entity str>,
{
    // Called only for entities recognized by the custom resolver; unknown and
    // predefined entities are handled by the caller.
    let replacement = resolve_entity(name)
        .ok_or_else(|| EscapeError::UnrecognizedEntity(0..name.len(), name.to_owned()))?;
    guard.reserve_bytes(replacement.len())?;

    // Reuse the text expansion logic with the shared guard for nested references.
    let expanded = expand_text(replacement, guard, &mut *resolve_entity)?;
    result.push_str(&expanded);
    Ok(())
}

/// Recursively unescapes `raw` with the given expansion limits.
pub(crate) fn unescape_with_limits<'input, 'entity, F>(
    raw: &'input str,
    expansion: EntityExpansion,
    resolve_entity: F,
) -> Result<Cow<'input, str>, EscapeError>
where
    F: FnMut(&str) -> Option<&'entity str>,
{
    let mut guard = expansion.guard();
    expand_text(raw, &mut guard, resolve_entity)
}
