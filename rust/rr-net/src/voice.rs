//! Voice packets: the Dissonance `VoiceData` packet the 2026 client sends (realtime.md section 7), carried as
//! NamedMessage 1 (to the server) and 2 (to clients) with an i32 length in front.
//!
//! Ours rides the room connection, not the stock client's second (voice) connection with its Dissonance server
//! handshake, sessions and channel tables: the server relays each packet to the rest of the room, and the sender id
//! is the actor number, stamped by the server. Field order is the spec's; big-endian fields are INFERRED (Dissonance
//! writes network order).
use crate::events::Malformed;

pub const TO_SERVER: u16 = 1;
pub const TO_CLIENT: u16 = 2;
const MAGIC: u16 = 0x8BC7;
const VOICE_DATA: u8 = 2;
/// Opus at 48 kHz mono, 20 ms frames (Dissonance "Small")
pub const SAMPLE_RATE: u32 = 48_000;
pub const FRAME: usize = 960;
/// ChannelBitField: room channel (type bit 0 clear), positional (bit 1)
const ROOM_POSITIONAL: u16 = 0b10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoicePacket {
    /// the speaker's actor number (set by the server)
    pub sender: u16,
    pub sequence: u16,
    /// loudness 0..255 (ChannelBitField bits 8-15)
    pub amplitude: u8,
    /// one Opus frame
    pub frame: Vec<u8>,
}

impl VoicePacket {
    /// The NamedMessage payload: i32 length, then the Dissonance packet.
    pub fn encode(&self) -> Vec<u8> {
        let mut p = vec![];
        p.extend_from_slice(&MAGIC.to_be_bytes());
        p.push(VOICE_DATA);
        p.extend_from_slice(&0u32.to_be_bytes()); // session
        p.extend_from_slice(&self.sender.to_be_bytes());
        p.push(0); // options: no channel session
        p.extend_from_slice(&self.sequence.to_be_bytes());
        p.extend_from_slice(&1u16.to_be_bytes()); // one channel: the room
        p.extend_from_slice(&(ROOM_POSITIONAL | (self.amplitude as u16) << 8).to_be_bytes());
        p.extend_from_slice(&0u16.to_be_bytes()); // recipient: the room
        p.extend_from_slice(&(self.frame.len() as u16).to_be_bytes());
        p.extend_from_slice(&self.frame);
        let mut out = (p.len() as i32).to_le_bytes().to_vec();
        out.extend(p);
        out
    }

    pub fn decode(b: &[u8]) -> Result<VoicePacket, Malformed> {
        let len = i32::from_le_bytes(b.get(..4).ok_or(Malformed)?.try_into().unwrap());
        let p = b.get(4..).filter(|p| len >= 0 && p.len() == len as usize).ok_or(Malformed)?;
        let u16_at = |i: usize| p.get(i..i + 2).map(|s| u16::from_be_bytes([s[0], s[1]])).ok_or(Malformed);
        if u16_at(0)? != MAGIC || p.get(2) != Some(&VOICE_DATA) {
            return Err(Malformed);
        }
        let (sender, sequence, channels) = (u16_at(7)?, u16_at(10)?, u16_at(12)? as usize);
        let amplitude = if channels > 0 { (u16_at(14)? >> 8) as u8 } else { 0 };
        let at = 14 + 4 * channels;
        let n = u16_at(at)? as usize;
        let frame = p.get(at + 2..at + 2 + n).filter(|_| at + 2 + n == p.len()).ok_or(Malformed)?.to_vec();
        Ok(VoicePacket { sender, sequence, amplitude, frame })
    }

    /// Rewrites the sender id inside an encoded packet (the server's stamp); false if it is not a voice packet.
    pub fn stamp(payload: &mut [u8], sender: u16) -> bool {
        if VoicePacket::decode(payload).is_err() {
            return false;
        }
        payload[4 + 7..4 + 9].copy_from_slice(&sender.to_be_bytes());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_packets_round_trip_and_get_stamped() {
        let v = VoicePacket { sender: 0, sequence: 65535, amplitude: 200, frame: vec![1, 2, 3, 250] };
        let mut b = v.encode();
        assert_eq!(&b[4..7], &[0x8B, 0xC7, 2]);
        assert_eq!(VoicePacket::decode(&b), Ok(v.clone()));
        assert!(VoicePacket::stamp(&mut b, 7));
        assert_eq!(VoicePacket::decode(&b), Ok(VoicePacket { sender: 7, ..v }));
        assert_eq!(VoicePacket::decode(&b[..b.len() - 1]), Err(Malformed));
        let mut junk = b.clone();
        junk[4] = 0;
        assert!(!VoicePacket::stamp(&mut junk, 1));
    }
}
