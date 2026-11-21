//! Wire framing shared by every Sony MDR headset generation.
//!
//! `>` ESCAPE(type, seq, len:u32be, payload…, checksum) `<`
//!
//! Ported from Plutoberth/SonyHeadphonesClient (`CommandSerializer.cpp`) and
//! abhiinhii/sony-tray (`Framing.cs`), both MIT.

pub const START: u8 = 0x3E;
pub const END: u8 = 0x3C;
const ESCAPE: u8 = 0x3D;

pub const TYPE_ACK: u8 = 0x01;
pub const TYPE_DATA_MDR: u8 = 0x0C;
pub const TYPE_DATA_MDR_NO2: u8 = 0x0E;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub kind: u8,
    pub seq: u8,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn is_ack(&self) -> bool {
        self.kind == TYPE_ACK
    }

    pub fn is_data(&self) -> bool {
        self.kind == TYPE_DATA_MDR || self.kind == TYPE_DATA_MDR_NO2
    }
}

fn checksum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |sum, b| sum.wrapping_add(*b))
}

fn escape_into(out: &mut Vec<u8>, data: &[u8]) {
    for &b in data {
        if matches!(b, 0x3C..=0x3E) {
            out.push(ESCAPE);
            out.push(b - 0x10);
        } else {
            out.push(b);
        }
    }
}

fn unescape(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len());
    let mut bytes = data.iter();
    while let Some(&b) = bytes.next() {
        if b == ESCAPE {
            match bytes.next() {
                Some(&next @ 0x2C..=0x2E) => out.push(next + 0x10),
                _ => return None,
            }
        } else {
            out.push(b);
        }
    }
    Some(out)
}

pub fn pack(kind: u8, seq: u8, payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(payload.len() + 7);
    body.push(kind);
    body.push(seq);
    body.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    body.extend_from_slice(payload);
    body.push(checksum(&body));

    let mut out = Vec::with_capacity(body.len() + 4);
    out.push(START);
    escape_into(&mut out, &body);
    out.push(END);
    out
}

/// Decodes one `>…<` delimited frame. Returns `None` for anything malformed.
pub fn unpack(packed: &[u8]) -> Option<Frame> {
    if packed.len() < 2 || packed[0] != START || packed[packed.len() - 1] != END {
        return None;
    }
    let body = unescape(&packed[1..packed.len() - 1])?;
    if body.len() < 7 {
        return None;
    }
    let (content, sum) = body.split_at(body.len() - 1);
    if checksum(content) != sum[0] {
        return None;
    }
    let declared = u32::from_be_bytes([body[2], body[3], body[4], body[5]]) as usize;
    if declared != body.len() - 7 {
        return None;
    }
    Some(Frame {
        kind: body[0],
        seq: body[1],
        payload: body[6..body.len() - 1].to_vec(),
    })
}

/// Accumulates raw socket bytes and yields complete frames, dropping garbage
/// between markers.
#[derive(Default)]
pub struct Reassembler {
    buffer: Vec<u8>,
}

impl Reassembler {
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<Frame> {
        self.buffer.extend_from_slice(chunk);
        let mut frames = Vec::new();
        loop {
            let Some(start) = self.buffer.iter().position(|b| *b == START) else {
                self.buffer.clear();
                break;
            };
            let Some(len) = self.buffer[start..].iter().position(|b| *b == END) else {
                self.buffer.drain(..start);
                break;
            };
            let end = start + len;
            if let Some(frame) = unpack(&self.buffer[start..=end]) {
                frames.push(frame);
            }
            self.buffer.drain(..=end);
        }
        frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_an_xm3_noise_command_like_the_original_client() {
        // SonyHeadphonesClient: NC on, seq 0.
        let payload = [0x68, 0x02, 0x11, 0x02, 0x02, 0x01, 0x00, 0x00];
        let packed = pack(TYPE_DATA_MDR, 0, &payload);
        assert_eq!(packed[0], START);
        assert_eq!(*packed.last().unwrap(), END);
        assert_eq!(&packed[1..7], &[0x0C, 0x00, 0x00, 0x00, 0x00, 0x08]);
        let sum = [0x0Cu8, 0, 0, 0, 0, 8]
            .iter()
            .chain(payload.iter())
            .fold(0u8, |a, b| a.wrapping_add(*b));
        assert_eq!(packed[packed.len() - 2], sum);
    }

    #[test]
    fn round_trips_payloads_containing_marker_bytes() {
        let payload = [0x3C, 0x3D, 0x3E, 0x00, 0xFF];
        let packed = pack(TYPE_DATA_MDR, 1, &payload);
        // Markers may only appear at the two ends.
        assert!(!packed[1..packed.len() - 1].contains(&START));
        assert!(!packed[1..packed.len() - 1].contains(&END));
        let frame = unpack(&packed).unwrap();
        assert_eq!(frame.kind, TYPE_DATA_MDR);
        assert_eq!(frame.seq, 1);
        assert_eq!(frame.payload, payload);
    }

    #[test]
    fn escapes_a_checksum_that_collides_with_a_marker() {
        // type 0x0C + seq 0 + len 1 + payload 0x2F = 0x3C, the end marker.
        let packed = pack(TYPE_DATA_MDR, 0, &[0x2F]);
        assert_eq!(&packed[packed.len() - 3..], &[ESCAPE, 0x2C, END]);
        assert_eq!(unpack(&packed).unwrap().payload, [0x2F]);
    }

    #[test]
    fn rejects_bad_checksum_length_and_escape() {
        let mut packed = pack(TYPE_DATA_MDR, 0, &[1, 2, 3]);
        let n = packed.len();
        packed[n - 2] ^= 0x01;
        assert!(unpack(&packed).is_none());

        let mut short = pack(TYPE_DATA_MDR, 0, &[1, 2, 3]);
        short[6] = 9; // declared length no longer matches
        assert!(unpack(&short).is_none());

        assert!(unpack(&[START, ESCAPE, 0x00, END]).is_none());
        assert!(unpack(&[START, END]).is_none());
    }

    #[test]
    fn ack_frames_are_empty() {
        let frame = unpack(&pack(TYPE_ACK, 1, &[])).unwrap();
        assert!(frame.is_ack());
        assert!(frame.payload.is_empty());
    }

    #[test]
    fn reassembles_split_and_back_to_back_frames() {
        let a = pack(TYPE_DATA_MDR, 0, &[0x11, 0x00, 0x50, 0x00]);
        let b = pack(TYPE_ACK, 1, &[]);
        let mut stream = vec![0xAA, 0xBB]; // leading noise
        stream.extend_from_slice(&a);
        stream.extend_from_slice(&b);

        let mut r = Reassembler::default();
        let (first, rest) = stream.split_at(5);
        assert!(r.feed(first).is_empty());
        let frames = r.feed(rest);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].payload, [0x11, 0x00, 0x50, 0x00]);
        assert!(frames[1].is_ack());
    }

    #[test]
    fn reassembler_drops_corrupt_frames_and_keeps_going() {
        let mut bad = pack(TYPE_DATA_MDR, 0, &[1, 2]);
        let n = bad.len();
        bad[n - 2] ^= 0x04;
        let good = pack(TYPE_DATA_MDR, 1, &[3, 4]);
        let mut r = Reassembler::default();
        let mut stream = bad;
        stream.extend_from_slice(&good);
        let frames = r.feed(&stream);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].payload, [3, 4]);
    }
}
