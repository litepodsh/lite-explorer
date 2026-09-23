//! PCM audio facts from a WAV header.
use super::{group, InfoSection};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

const WAV_MAX_CHUNKS: usize = 128;

pub(crate) fn section(file: &mut File) -> Option<InfoSection> {
    file.seek(SeekFrom::Start(12)).ok()?;
    for _ in 0..WAV_MAX_CHUNKS {
        let mut chunk = [0u8; 8];
        file.read_exact(&mut chunk).ok()?;
        let size = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        if &chunk[..4] == b"fmt " && size >= 16 {
            let mut format = [0u8; 16];
            file.read_exact(&mut format).ok()?;
            let mut section = InfoSection::new("Audio");
            section.push(
                "Channels",
                u16::from_le_bytes([format[2], format[3]]).to_string(),
            );
            let rate = u32::from_le_bytes([format[4], format[5], format[6], format[7]]);
            section.push("Sample rate", format!("{} Hz", group(u64::from(rate))));
            section.push(
                "Bits per sample",
                u16::from_le_bytes([format[14], format[15]]).to_string(),
            );
            return Some(section);
        }
        file.seek(SeekFrom::Current(i64::from(size) + i64::from(size % 2)))
            .ok()?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{rows, rows_for};

    #[test]
    fn wav_format_chunk() {
        let mut wav = b"RIFF\x24\0\0\0WAVE".to_vec();
        wav.extend(b"LIST\x02\0\0\0ab");
        wav.extend(b"fmt \x10\0\0\0");
        wav.extend(1u16.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(44_100u32.to_le_bytes());
        wav.extend(176_400u32.to_le_bytes());
        wav.extend(4u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        assert_eq!(
            rows_for(&wav, "a.wav", "Audio"),
            rows(&[
                ("Channels", "2"),
                ("Sample rate", "44,100 Hz"),
                ("Bits per sample", "16"),
            ])
        );
    }
}
