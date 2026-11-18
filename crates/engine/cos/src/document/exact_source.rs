//! Immutable source reads without substituting another object location.

use std::cell::RefCell;

use super::{Document, XrefEntry};
use crate::parser::parse_indirect_shared;
use crate::{CosError, Lexer, ObjRef, Object};

impl Document {
    /// Read a live uncompressed source object at its exact xref location, decrypted with
    /// this document's authenticated context. Ignores editing overlays and object caches;
    /// never shifts the offset or scans for another definition. Header number/generation
    /// must match without truncation. Indirect lengths use the same exact-source path,
    /// bounded to 16 dependency levels; compressed, missing or noninteger dependencies fail.
    ///
    /// This is a provenance check, not strict PDF syntax validation. The object parser's
    /// local tolerances (including direct stream-length/endstream recovery) still apply.
    /// Unencrypted stream payloads borrow the original bytes instead of being copied.
    pub fn try_get_original_at_xref(&self, reference: ObjRef) -> Result<Object, CosError> {
        self.original_at_xref(reference, 0)
    }

    fn original_at_xref(&self, reference: ObjRef, depth: usize) -> Result<Object, CosError> {
        if depth > 16 {
            return Err(CosError::Syntax { offset: 0, detail: "exact source length dependency is too deep or cyclic".into() });
        }
        let Some(XrefEntry::InFile { offset, generation }) = self.entries.get(&reference.num) else {
            return Err(CosError::MissingObject(reference.num));
        };
        let offset = usize::try_from(*offset).map_err(|_| CosError::MissingObject(reference.num))?;
        let invalid = || CosError::Syntax { offset, detail: "exact source object header does not match its live xref".into() };
        if *generation != reference.generation || offset >= self.data.len() {
            return Err(invalid());
        }
        // parse_indirect clamps oversized header integers. Check the original tokens first,
        // and require the xref to point at the header itself, not preceding whitespace.
        let mut header = Lexer::new(&self.data, offset);
        let number = header.token();
        header.skip_ws();
        let generation = header.token();
        let unsigned = |token: &[u8]| !token.is_empty() && token.iter().all(u8::is_ascii_digit);
        let parsed_number = std::str::from_utf8(number).ok().and_then(|s| s.parse::<u32>().ok());
        let parsed_generation = std::str::from_utf8(generation).ok().and_then(|s| s.parse::<u16>().ok());
        if !unsigned(number)
            || !unsigned(generation)
            || parsed_number != Some(reference.num)
            || parsed_generation != Some(reference.generation)
            || !header.eat_keyword(b"obj")
        {
            return Err(invalid());
        }
        // A failed resolver normally permits the parser to search for endstream. Retain
        // that error separately so damaged length dependencies cannot be silently repaired.
        let length_error = RefCell::new(None);
        let resolve_length = |length| match self.original_at_xref(length, depth + 1) {
            Ok(Object::Int(value)) if value >= 0 => Some(value),
            Ok(_) => {
                *length_error.borrow_mut() =
                    Some(CosError::Syntax { offset, detail: "exact source stream length is not a nonnegative integer".into() });
                None
            }
            Err(error) => {
                *length_error.borrow_mut() = Some(error);
                None
            }
        };
        let (parsed, object) = parse_indirect_shared(&self.data, offset, &resolve_length)?;
        if let Some(error) = length_error.into_inner() {
            return Err(error);
        }
        if parsed != reference {
            return Err(invalid());
        }
        Ok(self.decrypted(parsed, object))
    }
}
