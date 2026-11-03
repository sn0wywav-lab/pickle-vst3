//! RTP framing plus H.264 (RFC 6184) packetization.

pub const RTP_HEADER_LEN: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtpHeader {
    pub marker: bool,
    pub payload_type: u8,
    pub seq: u16,
    pub timestamp: u32,
    pub ssrc: u32,
}

impl RtpHeader {
    pub fn write(&self, out: &mut Vec<u8>, extension: Option<&[u8]>) {
        let x = if extension.is_some() { 0x10 } else { 0 };
        out.push(0x80 | x);
        out.push(((self.marker as u8) << 7) | (self.payload_type & 0x7F));
        out.extend_from_slice(&self.seq.to_be_bytes());
        out.extend_from_slice(&self.timestamp.to_be_bytes());
        out.extend_from_slice(&self.ssrc.to_be_bytes());
        if let Some(ext) = extension {
            out.extend_from_slice(ext);
        }
    }
}

/// Parsed view of an incoming (still encrypted) packet.
#[derive(Debug, Clone, Copy)]
pub struct RtpView {
    pub header: RtpHeader,
    /// Bytes covered by the AEAD as additional data (fixed header + CSRCs +
    /// the 4-byte extension preamble for `_rtpsize` modes).
    pub aad_len: usize,
    /// Length (bytes) of the extension body that sits at the start of the
    /// *encrypted* payload and must be skipped after decryption.
    pub ext_body_len: usize,
}

pub fn is_rtcp(packet: &[u8]) -> bool {
    packet.len() >= 2 && (200..=206).contains(&packet[1])
}

pub fn parse(packet: &[u8]) -> Option<RtpView> {
    if packet.len() < RTP_HEADER_LEN || packet[0] >> 6 != 2 {
        return None;
    }
    let cc = (packet[0] & 0x0F) as usize;
    let has_ext = packet[0] & 0x10 != 0;
    let header = RtpHeader {
        marker: packet[1] & 0x80 != 0,
        payload_type: packet[1] & 0x7F,
        seq: u16::from_be_bytes([packet[2], packet[3]]),
        timestamp: u32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]]),
        ssrc: u32::from_be_bytes([packet[8], packet[9], packet[10], packet[11]]),
    };
    let mut aad_len = RTP_HEADER_LEN + cc * 4;
    let mut ext_body_len = 0;
    if has_ext {
        if packet.len() < aad_len + 4 {
            return None;
        }
        let words = u16::from_be_bytes([packet[aad_len + 2], packet[aad_len + 3]]) as usize;
        aad_len += 4;
        ext_body_len = words * 4;
    }
    if packet.len() < aad_len {
        return None;
    }
    Some(RtpView { header, aad_len, ext_body_len })
}

/// One-byte header extension carrying `playout-delay` = 0 (id 5), which the
/// Discord SFU expects on video packets.
pub const VIDEO_HEADER_EXTENSION: [u8; 8] = [0xBE, 0xDE, 0x00, 0x01, 0x51, 0x00, 0x00, 0x00];

// ---------------------------------------------------------------------------
// H.264
// ---------------------------------------------------------------------------

/// Split an Annex-B byte stream into NAL units (without start codes).
pub fn split_annexb(data: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut start: Option<usize> = None;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            if let Some(s) = start {
                let mut end = i;
                if end > s && data[end - 1] == 0 {
                    end -= 1; // 4-byte start code
                }
                if end > s {
                    out.push(&data[s..end]);
                }
            }
            i += 3;
            start = Some(i);
        } else {
            i += 1;
        }
    }
    if let Some(s) = start {
        if s < data.len() {
            out.push(&data[s..]);
        }
    }
    out
}

/// Packetize one access unit. Calls `emit(payload, is_last)` per RTP packet.
pub fn packetize_h264(frame: &[u8], mtu: usize, mut emit: impl FnMut(&[u8], bool)) {
    let nals = split_annexb(frame);
    let count = nals.len();
    let mut buf = Vec::with_capacity(mtu + 2);
    for (idx, nal) in nals.into_iter().enumerate() {
        let last_nal = idx + 1 == count;
        if nal.is_empty() {
            continue;
        }
        if nal.len() <= mtu {
            emit(nal, last_nal);
            continue;
        }
        // FU-A
        let hdr = nal[0];
        let indicator = (hdr & 0xE0) | 28;
        let typ = hdr & 0x1F;
        let body = &nal[1..];
        let chunk = mtu - 2;
        let pieces = body.len().div_ceil(chunk);
        for (i, part) in body.chunks(chunk).enumerate() {
            let start = i == 0;
            let end = i + 1 == pieces;
            buf.clear();
            buf.push(indicator);
            buf.push(((start as u8) << 7) | ((end as u8) << 6) | typ);
            buf.extend_from_slice(part);
            emit(&buf, last_nal && end);
        }
    }
}

/// Reassembles access units from RTP payloads (single NAL, STAP-A, FU-A).
/// Output uses 4-byte start codes, which DAVE requires.
#[derive(Default)]
pub struct H264Depacketizer {
    frame: Vec<u8>,
    fu_active: bool,
    last_seq: Option<u16>,
    broken: bool,
    timestamp: Option<u32>,
}

const START_CODE: [u8; 4] = [0, 0, 0, 1];

impl H264Depacketizer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a payload. Returns a complete Annex-B frame when `marker` is set
    /// (or the timestamp changes) and no packets were lost.
    pub fn push(&mut self, seq: u16, timestamp: u32, marker: bool, payload: &[u8]) -> Option<Vec<u8>> {
        let mut out = None;
        if let Some(ts) = self.timestamp {
            if ts != timestamp && !self.frame.is_empty() {
                // previous frame never got its marker
                self.reset();
            }
        }
        if let Some(last) = self.last_seq {
            if seq != last.wrapping_add(1) {
                self.broken = true;
                self.fu_active = false;
            }
        }
        self.last_seq = Some(seq);
        self.timestamp = Some(timestamp);

        if !payload.is_empty() {
            let typ = payload[0] & 0x1F;
            match typ {
                1..=23 => {
                    self.frame.extend_from_slice(&START_CODE);
                    self.frame.extend_from_slice(payload);
                }
                24 => {
                    // STAP-A
                    let mut i = 1;
                    while i + 2 <= payload.len() {
                        let len = u16::from_be_bytes([payload[i], payload[i + 1]]) as usize;
                        i += 2;
                        if i + len > payload.len() {
                            self.broken = true;
                            break;
                        }
                        self.frame.extend_from_slice(&START_CODE);
                        self.frame.extend_from_slice(&payload[i..i + len]);
                        i += len;
                    }
                }
                28 if payload.len() >= 2 => {
                    let fu = payload[1];
                    let start = fu & 0x80 != 0;
                    if start {
                        self.frame.extend_from_slice(&START_CODE);
                        self.frame.push((payload[0] & 0xE0) | (fu & 0x1F));
                        self.fu_active = true;
                    } else if !self.fu_active {
                        self.broken = true;
                    }
                    self.frame.extend_from_slice(&payload[2..]);
                    if fu & 0x40 != 0 {
                        self.fu_active = false;
                    }
                }
                _ => {}
            }
        }

        if marker {
            if !self.broken && !self.frame.is_empty() {
                out = Some(std::mem::take(&mut self.frame));
            }
            self.reset();
        }
        out
    }

    fn reset(&mut self) {
        self.frame.clear();
        self.fu_active = false;
        self.broken = false;
    }
}

/// True if the access unit contains an IDR slice.
pub fn h264_is_keyframe(annexb: &[u8]) -> bool {
    split_annexb(annexb).iter().any(|n| !n.is_empty() && n[0] & 0x1F == 5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip_with_extension() {
        let h = RtpHeader { marker: true, payload_type: 101, seq: 7, timestamp: 9000, ssrc: 42 };
        let mut buf = Vec::new();
        h.write(&mut buf, Some(&VIDEO_HEADER_EXTENSION));
        buf.extend_from_slice(b"payload");
        let v = parse(&buf).unwrap();
        assert_eq!(v.header, h);
        assert_eq!(v.aad_len, 16);
        assert_eq!(v.ext_body_len, 4);
    }

    #[test]
    fn h264_roundtrip_fu_a() {
        let mut frame = Vec::new();
        frame.extend_from_slice(&[0, 0, 0, 1, 0x67, 1, 2, 3]); // SPS
        frame.extend_from_slice(&[0, 0, 0, 1, 0x68, 4, 5]); // PPS
        frame.extend_from_slice(&[0, 0, 0, 1, 0x65]);
        frame.extend((0..5000).map(|i| (i % 251) as u8 | 1)); // IDR, no start codes inside
        let mut packets = Vec::new();
        packetize_h264(&frame, 1000, |p, last| packets.push((p.to_vec(), last)));
        assert!(packets.len() > 5);
        assert!(packets.last().unwrap().1);
        assert_eq!(packets.iter().filter(|p| p.1).count(), 1);
        let mut d = H264Depacketizer::new();
        let mut out = None;
        for (i, (p, last)) in packets.iter().enumerate() {
            if let Some(f) = d.push(i as u16, 1234, *last, p) {
                out = Some(f);
            }
        }
        assert_eq!(out.unwrap(), frame);
        assert!(h264_is_keyframe(&frame));
    }

    #[test]
    fn depacketizer_drops_frames_with_loss() {
        let frame: Vec<u8> = [0u8, 0, 0, 1, 0x41].iter().copied().chain((0..3000).map(|_| 7u8)).collect();
        let mut packets = Vec::new();
        packetize_h264(&frame, 1000, |p, last| packets.push((p.to_vec(), last)));
        let mut d = H264Depacketizer::new();
        let mut got = None;
        for (i, (p, last)) in packets.iter().enumerate() {
            if i == 1 {
                continue;
            }
            got = got.or(d.push(i as u16, 1, *last, p));
        }
        assert!(got.is_none());
    }
}
