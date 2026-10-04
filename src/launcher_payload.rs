//! A bounded, versioned UTF-16 target appended to a native PE executable.
use std::io::{self, Read, Seek, SeekFrom, Write};

const MAGIC: &[u8; 16] = b"packslip-shim-v1";
const MAX_UNITS: usize = 32_767;

// The standalone launcher uses only read; the library generator uses only write.
#[allow(dead_code)]
pub fn write(writer: &mut impl Write, target: &[u16]) -> io::Result<()> {
    if target.is_empty() || target.len() > MAX_UNITS || target.contains(&0) {
        return Err(io::Error::other("invalid launcher target"));
    }
    for unit in target {
        writer.write_all(&unit.to_le_bytes())?;
    }
    writer.write_all(&(target.len() as u32).to_le_bytes())?;
    writer.write_all(MAGIC)
}

#[allow(dead_code)]
pub fn read(reader: &mut (impl Read + Seek)) -> io::Result<Vec<u16>> {
    let size = reader.seek(SeekFrom::End(0))?;
    if size < 20 {
        return Err(io::Error::other("missing launcher target"));
    }
    reader.seek(SeekFrom::End(-20))?;
    let mut length = [0; 4];
    let mut magic = [0; 16];
    reader.read_exact(&mut length)?;
    reader.read_exact(&mut magic)?;
    let units = u32::from_le_bytes(length) as usize;
    if &magic != MAGIC || units == 0 || units > MAX_UNITS || units as u64 * 2 + 20 >= size {
        return Err(io::Error::other("invalid launcher target footer"));
    }
    reader.seek(SeekFrom::End(-20 - units as i64 * 2))?;
    let mut bytes = vec![0; units * 2];
    reader.read_exact(&mut bytes)?;
    let target: Vec<_> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    if target.contains(&0) {
        return Err(io::Error::other("launcher target contains NUL"));
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_utf16_and_rejects_bad_footers() {
        let target = [b'C' as u16, b':' as u16, b'\\' as u16, 0xd800, 0x2603];
        let mut bytes = b"PE executable".to_vec();
        write(&mut bytes, &target).unwrap();
        assert_eq!(read(&mut io::Cursor::new(&bytes)).unwrap(), target);
        for cut in 0..bytes.len() {
            assert!(read(&mut io::Cursor::new(&bytes[..cut])).is_err());
        }
        *bytes.last_mut().unwrap() = 1;
        assert!(read(&mut io::Cursor::new(bytes)).is_err());
        assert!(write(&mut Vec::new(), &[0]).is_err());
        assert!(write(&mut Vec::new(), &vec![1; MAX_UNITS + 1]).is_err());
    }
}
