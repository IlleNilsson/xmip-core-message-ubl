#![forbid(unsafe_code)]

//! UBL: an XML document in the OASIS Universal Business Language, one part,
//! with the document type it announces — `Invoice`, `Order`, `DespatchAdvice`
//! — as its type. UBL is an XML vocabulary, so the document is read through
//! the `xml` shape's walk; what is this technology's own is the namespace a
//! UBL 2 document declares on its root,
//! `urn:oasis:names:specification:ubl:schema:xsd:<Type>-2`, and the refusal
//! of a root that is not in it (ADR-0047).
//!
//! No media type is registered for UBL — it travels as `application/xml` —
//! so this shape claims none and is known by its namespace when the bytes
//! are asked. Put it before `xml` in the order `choose` is given.

use message::{Part, Shape, ShapeError, Shaped};
use message_xml::root::{self, Root};
use stream::Stream;

/// The UBL shape.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ubl;

/// What every UBL 2 document namespace begins with.
const NAMESPACE_PREFIX: &str = "urn:oasis:names:specification:ubl:schema:xsd:";

/// The document type when `root` is a UBL 2 root: its local name, which the
/// namespace names too.
fn document_type(root: &Root) -> Option<&str> {
    let namespace = root.namespace.as_deref()?;
    let named = namespace
        .strip_prefix(NAMESPACE_PREFIX)?
        .strip_suffix("-2")?;
    (named == root.local_name).then_some(named)
}

impl Shape for Ubl {
    fn technology(&self) -> &'static str {
        "ubl"
    }

    fn media_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn recognises(&self, bytes: &[u8]) -> bool {
        root::document(bytes).is_ok_and(|root| document_type(&root).is_some())
    }

    fn shape(&self, stream: &Stream) -> Result<Shaped, ShapeError> {
        let root =
            root::document(stream.bytes()).map_err(|stop| ShapeError::refused("ubl", stop))?;
        let Some(document_type) = document_type(&root) else {
            return Err(
                ShapeError::new("ubl", "the root element is not in a UBL 2 namespace")
                    .at(root.offset),
            );
        };
        let media = stream
            .media_type()
            .map_or_else(|| "application/xml".to_string(), str::to_string);
        Ok(Shaped {
            parts: vec![Part::new(None, stream.bytes(), Some(media))],
            message_type: Some(document_type.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xcore::StreamId;

    const INVOICE: &[u8] = b"<?xml version=\"1.0\"?>\n<Invoice \
xmlns=\"urn:oasis:names:specification:ubl:schema:xsd:Invoice-2\" \
xmlns:cbc=\"urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2\">\
<cbc:ID>INV-1</cbc:ID></Invoice>";

    fn stream(bytes: &[u8], media: Option<&str>) -> Stream {
        Stream::new(StreamId::new(1), bytes.to_vec(), media.map(str::to_string))
    }

    #[test]
    fn a_ubl_document_is_one_part_and_its_document_type_is_the_announced_type() {
        let shaped = Ubl.shape(&stream(INVOICE, None)).expect("UBL");
        assert_eq!(shaped.parts.len(), 1);
        assert_eq!(shaped.parts[0].bytes, INVOICE);
        assert_eq!(
            shaped.parts[0].media_type.as_deref(),
            Some("application/xml")
        );
        assert_eq!(shaped.message_type.as_deref(), Some("Invoice"));

        let prefixed =
            b"<o:Order xmlns:o=\"urn:oasis:names:specification:ubl:schema:xsd:Order-2\"/>";
        let order = Ubl
            .shape(&stream(prefixed, Some("application/xml")))
            .expect("UBL");
        assert_eq!(order.message_type.as_deref(), Some("Order"));
    }

    #[test]
    fn a_root_outside_the_ubl_namespace_and_a_broken_document_are_refused_where_they_fail() {
        let elsewhere = Ubl
            .shape(&stream(b"\n<Invoice xmlns=\"urn:x:invoice\"/>", None))
            .expect_err("not UBL");
        assert_eq!(elsewhere.offset, Some(1));
        assert_eq!(
            elsewhere.to_string(),
            "ubl: the root element is not in a UBL 2 namespace at byte 1"
        );

        let bare = Ubl
            .shape(&stream(b"<Invoice/>", None))
            .expect_err("no namespace");
        assert_eq!(bare.offset, Some(0));

        let mismatched =
            b"<Order xmlns=\"urn:oasis:names:specification:ubl:schema:xsd:Invoice-2\"/>";
        assert!(Ubl.shape(&stream(mismatched, None)).is_err());

        let broken = Ubl
            .shape(&stream(b"<Invoice xmlns=\"urn:x\">", None))
            .expect_err("never closed");
        assert_eq!(broken.technology, "ubl");
        assert_eq!(broken.offset, Some(23));
    }

    #[test]
    fn the_shape_claims_no_media_type_and_recognises_a_ubl_root() {
        assert_eq!(Ubl.technology(), "ubl");
        assert!(Ubl.media_types().is_empty());
        assert!(Ubl.recognises(INVOICE));
        assert!(!Ubl.recognises(b"<Invoice/>"));
        assert!(!Ubl.recognises(b"{\"type\": \"Invoice\"}"));
    }

    #[test]
    fn choose_takes_ubl_by_look_before_xml_and_leaves_other_xml_to_xml() {
        let shapes: [&dyn Shape; 2] = [&Ubl, &message_xml::Xml];
        let ubl = message::choose(&shapes, &stream(INVOICE, None));
        assert_eq!(ubl.map(Shape::technology), Some("ubl"));
        let xml = message::choose(&shapes, &stream(b"<a/>", None));
        assert_eq!(xml.map(Shape::technology), Some("xml"));
        let by_media = message::choose(&shapes, &stream(INVOICE, Some("application/xml")));
        assert_eq!(by_media.map(Shape::technology), Some("xml"));
    }
}
