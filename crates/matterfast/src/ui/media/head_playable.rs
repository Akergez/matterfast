/// Whether `head` — the first slice of a file, not necessarily all of it —
/// contains enough of the container's own index to build a poster from
/// without fetching any more of the file.
///
/// WebM/Matroska (EBML) is always worth trying: both mux their content as a
/// sequence of clusters that a demuxer can start reading from the front, so
/// there is no index that has to arrive first. An ISO base media file
/// (MP4/MOV/M4V) is the opposite: nothing can be decoded until `moov` (the
/// sample table) is in hand, and an encoder that skips `-movflags
/// +faststart` writes `mdat` — the (large) media itself — before it, putting
/// `moov` at the very end of the file. [`mp4_head_has_full_moov`] is the
/// cheap check for that; failing it means giving up quietly rather than
/// fetching the rest of a multi-hundred-MB file for a thumbnail.
pub fn head_playable(head: &[u8]) -> bool {
    // The four bytes every EBML document — WebM included — starts with.
    const EBML_MAGIC: [u8; 4] = [0x1A, 0x45, 0xDF, 0xA3];
    head.starts_with(&EBML_MAGIC) || mp4_head_has_full_moov(head)
}

/// Walks the top-level ISO-BMFF boxes in `head` (`[u32 size][4cc type][...]`,
/// or a `u64` size when the 32-bit one reads as `1`) looking for a `moov`
/// whose declared size ends inside what was fetched.
///
/// For a non-faststart file this runs out of `head` while still inside the
/// leading `mdat` — which, being the whole video, is almost always bigger
/// than `head` itself — and returns `false` well before reaching the real
/// `moov` at the end. That is the giving-up case, not a bug: the caller does
/// not fetch further to find out for certain.
fn mp4_head_has_full_moov(head: &[u8]) -> bool {
    let mut pos = 0usize;
    while pos + 8 <= head.len() {
        let size32 = u32::from_be_bytes(head[pos..pos + 4].try_into().unwrap()) as u64;
        let kind = &head[pos + 4..pos + 8];

        let (header_len, size) = if size32 == 1 {
            // The real size is a 64-bit field right after the ordinary
            // header; a box needing one is rare enough that the head simply
            // not containing it is treated the same as not finding `moov`.
            if pos + 16 > head.len() {
                break;
            }
            (
                16u64,
                u64::from_be_bytes(head[pos + 8..pos + 16].try_into().unwrap()),
            )
        } else if size32 == 0 {
            // "Runs to the end of the file" — never true of a box we still
            // need to skip past to reach `moov` within a bounded head.
            break;
        } else {
            (8, size32)
        };

        if kind == b"moov" {
            return pos as u64 + size <= head.len() as u64;
        }
        if size < header_len {
            break; // malformed box: an infinite loop is worse than giving up
        }
        match pos.checked_add(size as usize) {
            Some(next) => pos = next,
            None => break,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A single ISO-BMFF box: 4-byte size (including this header) + 4-byte
    /// type + `payload`.
    fn mp4_box(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + payload.len());
        out.extend_from_slice(&((8 + payload.len()) as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }

    /// `ffmpeg -movflags +faststart` on a real clip lays out `ftyp`, `moov`,
    /// `free`, `mdat`, in that order (verified against `/tmp/mmtest.mp4`) —
    /// `moov` is small and near the front, so a bounded head contains it whole.
    #[test]
    fn a_faststart_head_has_a_full_moov() {
        let mut head = mp4_box(b"ftyp", &[0; 24]);
        head.extend(mp4_box(b"moov", &[0; 200]));
        // The rest of the file (`mdat`) need not even be present.
        assert!(head_playable(&head));
    }

    /// Without `+faststart`, `ffmpeg` instead writes `ftyp`, `free`, `mdat`,
    /// `moov` — `mdat` is the whole video, so a head bounded well short of
    /// the file's end never reaches the trailing `moov`.
    #[test]
    fn a_streaming_unfriendly_head_gives_up() {
        let mut head = mp4_box(b"ftyp", &[0; 24]);
        head.extend(mp4_box(b"free", &[]));
        // Declares a size far bigger than what's actually in `head` — the
        // stand-in for "the rest of a multi-hundred-MB clip".
        head.extend(50u32.to_be_bytes());
        head.extend(b"mdat");
        // No `moov` ever follows within this slice.
        assert!(!head_playable(&head));
    }

    /// `moov`'s header is in the head, but its declared size runs past the
    /// end of what was actually fetched — an incomplete atom is as useless
    /// as no atom, and must not be read out of bounds either.
    #[test]
    fn a_moov_box_cut_off_mid_atom_gives_up() {
        let mut head = mp4_box(b"ftyp", &[0; 24]);
        head.extend(200u32.to_be_bytes());
        head.extend(b"moov");
        head.extend([0; 50]); // far short of the 200 the header promises
        assert!(!head_playable(&head));
    }

    /// WebM/Matroska never needs the box scan at all — only the magic bytes.
    #[test]
    fn a_webm_head_is_always_playable() {
        let mut head = vec![0x1A, 0x45, 0xDF, 0xA3];
        head.extend([0; 16]); // arbitrary EBML content
        assert!(head_playable(&head));
    }

    #[test]
    fn empty_or_garbage_is_not_playable() {
        assert!(!head_playable(&[]));
        assert!(!head_playable(b"not a media file at all"));
    }
}
