//! A PDF with one picture per page, for `render --slides`.

use std::io::Write;

use oxframe_core::Image;

/// Points per pixel: 1920 pixels become a 1440-point (20-inch) page, the
/// size a 96-dpi screen shows at 100 %.
const PT_PER_PX: f64 = 0.75;

/// A PDF whose pages are `frames`, each filling its page. Transparent
/// pixels are drawn over white.
pub(crate) fn pages(frames: &[Image]) -> Vec<u8> {
    let mut out = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    let n = frames.len();
    // Objects: 1 catalog, 2 page tree, then per page: page, content, image.
    let page_id = |i: usize| 3 + 3 * i;
    let mut object = |out: &mut Vec<u8>, id: usize, body: &[u8]| {
        offsets.push((id, out.len()));
        out.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };
    object(&mut out, 1, b"<< /Type /Catalog /Pages 2 0 R >>");
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", page_id(i))).collect();
    object(
        &mut out,
        2,
        format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")).as_bytes(),
    );
    for (i, frame) in frames.iter().enumerate() {
        let (w, h) = (
            f64::from(frame.width) * PT_PER_PX,
            f64::from(frame.height) * PT_PER_PX,
        );
        let (page, content, image) = (page_id(i), page_id(i) + 1, page_id(i) + 2);
        object(
            &mut out,
            page,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w:.2} {h:.2}] \
                 /Resources << /XObject << /Im0 {image} 0 R >> >> /Contents {content} 0 R >>"
            )
            .as_bytes(),
        );
        let draw = format!("q {w:.2} 0 0 {h:.2} 0 0 cm /Im0 Do Q");
        object(
            &mut out,
            content,
            format!("<< /Length {} >>\nstream\n{draw}\nendstream", draw.len()).as_bytes(),
        );
        let pixels = deflate(&rgb_over_white(frame));
        let mut body = format!(
            "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB \
             /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
            frame.width,
            frame.height,
            pixels.len()
        )
        .into_bytes();
        body.extend_from_slice(&pixels);
        body.extend_from_slice(b"\nendstream");
        object(&mut out, image, &body);
    }
    offsets.sort_unstable();
    let xref = out.len();
    let count = offsets.len() + 1;
    out.extend_from_slice(format!("xref\n0 {count}\n0000000000 65535 f \n").as_bytes());
    for (_, at) in &offsets {
        out.extend_from_slice(format!("{at:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {count} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    out
}

fn rgb_over_white(frame: &Image) -> Vec<u8> {
    frame
        .data
        .chunks_exact(4)
        .flat_map(|p| {
            let a = u16::from(p[3]);
            let over = |c: u8| ((u16::from(c) * a + 255 * (255 - a)) / 255) as u8;
            [over(p[0]), over(p[1]), over(p[2])]
        })
        .collect()
}

fn deflate(bytes: &[u8]) -> Vec<u8> {
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(bytes).expect("writing to memory");
    z.finish().expect("writing to memory")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
        hay.windows(needle.len()).rposition(|w| w == needle)
    }

    #[test]
    fn one_page_per_frame_with_a_valid_xref() {
        let frame = |c: u8| Image {
            width: 4,
            height: 2,
            data: [c, c, c, 255].repeat(8),
        };
        let pdf = pages(&[frame(0), frame(200)]);
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(find(&pdf, b"/Count 2").is_some());
        assert!(find(&pdf, b"/MediaBox [0 0 3.00 1.50]").is_some());
        // Every xref offset points at the start of its object.
        let tail = &pdf[find(&pdf, b"startxref\n").unwrap() + 10..];
        let xref: usize = std::str::from_utf8(tail)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let table = std::str::from_utf8(&pdf[xref..]).unwrap();
        let entries: Vec<&str> = table.lines().skip(3).take(8).collect();
        assert_eq!(entries.len(), 8);
        for (k, line) in entries.iter().enumerate() {
            let at: usize = line[..10].parse().unwrap();
            let head = format!("{} 0 obj", k + 1);
            assert!(pdf[at..].starts_with(head.as_bytes()), "object {}", k + 1);
        }
    }

    #[test]
    fn transparent_pixels_are_white() {
        let img = Image {
            width: 1,
            height: 1,
            data: vec![0, 0, 0, 0],
        };
        assert_eq!(rgb_over_white(&img), [255, 255, 255]);
    }
}
