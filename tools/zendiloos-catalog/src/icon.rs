use crate::Problem;

pub const ICON_FILE: &str = "icon.png";
pub const MAX_ICON_BYTES: usize = 256 * 1024;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

/// Checks the icon: a square PNG from 256 to 1024 pixels. PNG rather than
/// SVG, because an SVG may carry scripts.
pub fn check_icon(bytes: &[u8]) -> Vec<Problem> {
    if bytes.len() > MAX_ICON_BYTES {
        return vec![Problem::icon(format!("must be at most {} KiB", MAX_ICON_BYTES / 1024))];
    }
    // The first chunk of a PNG is IHDR, which starts with width and height.
    let header = bytes.get(..24).filter(|header| header[..8] == PNG_SIGNATURE && &header[12..16] == b"IHDR");
    let Some(header) = header else {
        return vec![Problem::icon("is not a PNG image")];
    };
    let width = u32::from_be_bytes([header[16], header[17], header[18], header[19]]);
    let height = u32::from_be_bytes([header[20], header[21], header[22], header[23]]);
    if width != height {
        vec![Problem::icon(format!("must be square, but is {width}×{height} pixels"))]
    } else if !(256..=1024).contains(&width) {
        vec![Problem::icon(format!("must be 256 to 1024 pixels wide, but is {width}"))]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes
    }

    #[test]
    fn square_pngs_of_a_sensible_size_pass() {
        assert!(check_icon(&png(256, 256)).is_empty());
        assert!(check_icon(&png(512, 512)).is_empty());
    }

    #[test]
    fn other_images_are_refused() {
        assert_eq!(check_icon(b"<svg/>")[0].message, "is not a PNG image");
        assert_eq!(check_icon(&png(256, 128))[0].message, "must be square, but is 256×128 pixels");
        assert_eq!(check_icon(&png(64, 64))[0].message, "must be 256 to 1024 pixels wide, but is 64");
    }
}
