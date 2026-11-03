//! The XMP packet that carries a picture's details (caption, people, place, date) in the standard
//! fields photo programs read. Built by hand: nothing else in the project needs an XMP library, and
//! the ones that exist build C++ code.

use crate::details::Details;

/// What a JPEG APP1 segment starts with when it holds XMP.
pub const JPEG_HEADER: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
/// The keyword of the PNG `iTXt` chunk that holds XMP.
pub const PNG_KEYWORD: &str = "XML:com.adobe.xmp";
/// The TIFF tag that holds XMP.
pub const TIFF_TAG: u16 = 700;
/// The namespace of the one field with no standard home: the date as the user wrote it.
pub const OXICUT_NAMESPACE: &str = "https://github.com/oxicut/oxicut/ns/1.0/";

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

fn bag(out: &mut String, element: &str, items: &[String]) {
    out.push_str(&format!("   <{element}><rdf:Bag>\n"));
    for item in items {
        out.push_str(&format!("    <rdf:li>{}</rdf:li>\n", escape(item)));
    }
    out.push_str(&format!("   </rdf:Bag></{element}>\n"));
}

/// The XMP packet for `details`, or `None` when there is nothing to say. `software` (`OxiCut 0.0.1`)
/// is named as the creator tool when the user wants OxiCut named.
pub fn packet(details: &Details, software: Option<&str>) -> Option<Vec<u8>> {
    let details = details.normalized();
    if details.is_empty() {
        return None;
    }
    let mut attributes = String::new();
    let mut attribute = |name: &str, value: &str| {
        if !value.is_empty() {
            attributes.push_str(&format!("\n    {name}=\"{}\"", escape(value)));
        }
    };
    attribute("photoshop:DateCreated", &details.iso_date().unwrap_or_default());
    attribute("Iptc4xmpCore:Location", &details.place);
    attribute("oxicut:dateText", &details.date_text);
    attribute("xmp:CreatorTool", software.unwrap_or_default());

    let mut out = String::from("<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n");
    out.push_str("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n");
    out.push_str(" <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n");
    out.push_str("  <rdf:Description rdf:about=\"\"\n");
    out.push_str("    xmlns:dc=\"http://purl.org/dc/elements/1.1/\"\n");
    out.push_str("    xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"\n");
    out.push_str("    xmlns:photoshop=\"http://ns.adobe.com/photoshop/1.0/\"\n");
    out.push_str("    xmlns:Iptc4xmpCore=\"http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/\"\n");
    out.push_str("    xmlns:Iptc4xmpExt=\"http://iptc.org/std/Iptc4xmpExt/2008-02-29/\"\n");
    out.push_str(&format!("    xmlns:oxicut=\"{OXICUT_NAMESPACE}\""));
    out.push_str(&attributes);
    out.push_str(">\n");
    if !details.caption.is_empty() {
        out.push_str(&format!(
            "   <dc:description><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:description>\n",
            escape(&details.caption)
        ));
    }
    if !details.people.is_empty() {
        // The names are both keywords (every photo program shows those) and the standard "person shown".
        bag(&mut out, "dc:subject", &details.people);
        bag(&mut out, "Iptc4xmpExt:PersonInImage", &details.people);
    }
    out.push_str("  </rdf:Description>\n </rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"w\"?>");
    Some(out.into_bytes())
}

/// The body of a JPEG APP1 segment holding `packet`. `None` when it would not fit (a segment holds
/// 64 KB; the limits on the details keep real packets far below that).
pub fn jpeg_segment(packet: &[u8]) -> Option<Vec<u8>> {
    let mut body = JPEG_HEADER.to_vec();
    body.extend_from_slice(packet);
    (body.len() + 2 <= 65_535).then_some(body)
}

/// The data of a PNG `iTXt` chunk holding `packet`: keyword, no compression, no language, then the text.
pub fn png_itxt(packet: &[u8]) -> Vec<u8> {
    let mut data = PNG_KEYWORD.as_bytes().to_vec();
    data.extend_from_slice(&[0, 0, 0, 0, 0]); // end of keyword, flag, method, language, translated keyword
    data.extend_from_slice(packet);
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(details: &Details, software: Option<&str>) -> String {
        String::from_utf8(packet(details, software).unwrap()).unwrap()
    }

    #[test]
    fn everything_the_user_entered_is_in_the_packet() {
        let xml = text(
            &Details {
                date_text: "summer 1975".into(),
                year: Some(1975),
                place: "Balaton, Hungary".into(),
                caption: "Beach day".into(),
                people: vec!["Anna".into(), "Péter".into()],
                ..Default::default()
            },
            Some("OxiCut 0.0.1"),
        );
        assert!(xml.starts_with("<?xpacket begin=\"\u{feff}\""));
        assert!(xml.ends_with("<?xpacket end=\"w\"?>"));
        assert!(xml.contains("photoshop:DateCreated=\"1975\""));
        assert!(xml.contains("Iptc4xmpCore:Location=\"Balaton, Hungary\""));
        assert!(xml.contains("oxicut:dateText=\"summer 1975\""));
        assert!(xml.contains("xmp:CreatorTool=\"OxiCut 0.0.1\""));
        assert!(xml.contains("<rdf:li xml:lang=\"x-default\">Beach day</rdf:li>"));
        assert!(xml.contains("<dc:subject><rdf:Bag>\n    <rdf:li>Anna</rdf:li>\n    <rdf:li>Péter</rdf:li>"));
        assert!(xml.contains("<Iptc4xmpExt:PersonInImage><rdf:Bag>"));
    }

    #[test]
    fn what_was_not_entered_is_not_written() {
        let xml = text(
            &Details {
                caption: "Only a caption".into(),
                ..Default::default()
            },
            None,
        );
        assert!(!xml.contains("DateCreated"));
        assert!(!xml.contains("Location"));
        assert!(!xml.contains("CreatorTool"));
        assert!(!xml.contains("dc:subject"));
        assert!(!xml.contains("PersonInImage"));
        assert!(xml.contains("Only a caption"));
    }

    #[test]
    fn markup_in_names_and_captions_cannot_break_the_packet() {
        let xml = text(
            &Details {
                caption: "Tom & \"Jerry\" <3 it's".into(),
                people: vec!["A&B <C>".into()],
                place: "\"x\" & 'y'".into(),
                ..Default::default()
            },
            None,
        );
        assert!(xml.contains("Tom &amp; &quot;Jerry&quot; &lt;3 it&apos;s"));
        assert!(xml.contains("<rdf:li>A&amp;B &lt;C&gt;</rdf:li>"));
        assert!(xml.contains("Iptc4xmpCore:Location=\"&quot;x&quot; &amp; &apos;y&apos;\""));
        assert!(!xml.contains("<C>"));
    }

    #[test]
    fn no_details_means_no_packet_even_with_the_software_stamp() {
        assert!(packet(&Details::default(), Some("OxiCut 0.0.1")).is_none());
        assert!(packet(&Details { place: "  ".into(), ..Default::default() }, None).is_none());
    }

    #[test]
    fn the_longest_allowed_details_still_fit_a_jpeg_segment() {
        let big = Details {
            caption: "é".repeat(crate::details::MAX_CAPTION),
            place: "p".repeat(crate::details::MAX_TEXT),
            date_text: "d".repeat(crate::details::MAX_TEXT),
            year: Some(1975),
            people: (0..crate::details::MAX_PEOPLE).map(|i| format!("{i:0>200}")).collect(),
            ..Default::default()
        };
        let packet = packet(&big, Some("OxiCut 0.0.1")).unwrap();
        let segment = jpeg_segment(&packet);
        assert!(segment.is_some(), "{} bytes", packet.len());
        assert!(jpeg_segment(&vec![b'x'; 70_000]).is_none());
        assert!(segment.unwrap().starts_with(JPEG_HEADER));
    }

    #[test]
    fn the_png_chunk_has_the_keyword_flags_and_text() {
        let data = png_itxt(b"<x/>");
        assert!(data.starts_with(b"XML:com.adobe.xmp\0\0\0\0\0<x/>"));
    }
}
