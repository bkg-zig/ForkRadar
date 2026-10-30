//! Minimal writer for ZIP containers.
//!
//! The XLSX format is a ZIP archive with XML members. The writer stores the
//! members without compression, which keeps the implementation small and the
//! output readable by every spreadsheet program.

/// Computes the CRC-32 checksum of a byte slice.
///
/// The polynomial is the one of the ZIP format, the result is the same as the
/// checksum of `crc32` from the zlib library.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

/// One member of the archive.
#[derive(Debug)]
struct Member {
    name: String,
    data: Vec<u8>,
}

/// Builds a ZIP archive with stored members.
#[derive(Debug, Default)]
pub struct ZipWriter {
    members: Vec<Member>,
}

impl ZipWriter {
    /// Creates an empty archive.
    pub fn new() -> Self {
        Self {
            members: Vec::new(),
        }
    }

    /// Adds a member under the given path.
    pub fn add(&mut self, name: impl Into<String>, data: impl Into<Vec<u8>>) {
        self.members.push(Member {
            name: name.into(),
            data: data.into(),
        });
    }

    /// Writes the archive.
    ///
    /// The members are stored without compression. The central directory holds
    /// the offsets and checksums that a reader needs.
    pub fn finish(self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        let mut offsets = Vec::new();

        for member in &self.members {
            let offset = out.len() as u32;
            offsets.push(offset);
            let checksum = crc32(&member.data);
            let name = member.name.as_bytes();

            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&checksum.to_le_bytes());
            out.extend_from_slice(&(member.data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(member.data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name);
            out.extend_from_slice(&member.data);
        }

        for (index, member) in self.members.iter().enumerate() {
            let checksum = crc32(&member.data);
            let name = member.name.as_bytes();
            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&checksum.to_le_bytes());
            central.extend_from_slice(&(member.data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(member.data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&offsets[index].to_le_bytes());
            central.extend_from_slice(name);
        }

        let central_offset = out.len() as u32;
        let central_size = central.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(self.members.len() as u16).to_le_bytes());
        out.extend_from_slice(&(self.members.len() as u16).to_le_bytes());
        out.extend_from_slice(&central_size.to_le_bytes());
        out.extend_from_slice(&central_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_the_known_test_vector() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn the_archive_carries_the_end_of_central_directory_marker() {
        let mut writer = ZipWriter::new();
        writer.add("a.txt", b"hello");
        writer.add("b.txt", b"world");
        let bytes = writer.finish();
        assert_eq!(&bytes[0..4], &0x0403_4b50u32.to_le_bytes());
        let tail = &bytes[bytes.len() - 22..];
        assert_eq!(&tail[0..4], &0x0605_4b50u32.to_le_bytes());
        assert_eq!(u16::from_le_bytes([tail[10], tail[11]]), 2);
    }

    #[test]
    fn the_stored_member_keeps_its_payload() {
        let mut writer = ZipWriter::new();
        writer.add("a.txt", b"hello");
        let bytes = writer.finish();
        let position = bytes
            .windows(5)
            .position(|window| window == b"a.txt")
            .expect("member name");
        assert_eq!(&bytes[position + 5..position + 10], b"hello");
    }
}
