//! Original SHA256 rounds, shared by legacy one-shot and paid canonical streaming.
use crate::CanonicalCborStreamError as Error;
pub(super) struct Sha256V1 {
    state: [u32; 8],
    block: [u8; 64],
    used: usize,
    bytes: u64,
}
impl Sha256V1 {
    pub(super) fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            block: [0; 64],
            used: 0,
            bytes: 0,
        }
    }
    pub(super) fn update(
        &mut self,
        mut input: &[u8],
        work: &mut dyn FnMut(u64, u64) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.bytes = self.bytes.wrapping_add(input.len() as u64);
        while !input.is_empty() {
            let n = (64 - self.used).min(input.len());
            work(1, n as u64)?;
            self.block[self.used..self.used + n].copy_from_slice(&input[..n]);
            self.used += n;
            input = &input[n..];
            if self.used == 64 {
                work(64, 64)?;
                compress(&mut self.state, &self.block);
                self.used = 0
            }
        }
        Ok(())
    }
    pub(super) fn finish(
        mut self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), Error>,
    ) -> Result<[u8; 32], Error> {
        let bits = self.bytes.wrapping_mul(8).to_be_bytes();
        let n = if self.used < 56 {
            64 - self.used
        } else {
            128 - self.used
        };
        let mut padding = [0u8; 128];
        padding[0] = 0x80;
        padding[n - 8..n].copy_from_slice(&bits);
        self.update(&padding[..n], work)?;
        work(8, 32)?;
        let mut out = [0u8; 32];
        for (i, v) in self.state.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes())
        }
        Ok(out)
    }
}
fn compress(state: &mut [u32; 8], chunk: &[u8; 64]) {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut schedule = [0_u32; 64];
    for (index, bytes) in chunk.chunks_exact(4).take(16).enumerate() {
        schedule[index] = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    }
    for index in 16..64 {
        let s0 = schedule[index - 15].rotate_right(7)
            ^ schedule[index - 15].rotate_right(18)
            ^ (schedule[index - 15] >> 3);
        let s1 = schedule[index - 2].rotate_right(17)
            ^ schedule[index - 2].rotate_right(19)
            ^ (schedule[index - 2] >> 10);
        schedule[index] = schedule[index - 16]
            .wrapping_add(s0)
            .wrapping_add(schedule[index - 7])
            .wrapping_add(s1);
    }
    let mut working = *state;
    for index in 0..64 {
        let s1 =
            working[4].rotate_right(6) ^ working[4].rotate_right(11) ^ working[4].rotate_right(25);
        let ch = (working[4] & working[5]) ^ ((!working[4]) & working[6]);
        let temp1 = working[7]
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[index])
            .wrapping_add(schedule[index]);
        let s0 =
            working[0].rotate_right(2) ^ working[0].rotate_right(13) ^ working[0].rotate_right(22);
        let maj = (working[0] & working[1]) ^ (working[0] & working[2]) ^ (working[1] & working[2]);
        let temp2 = s0.wrapping_add(maj);
        working[7] = working[6];
        working[6] = working[5];
        working[5] = working[4];
        working[4] = working[3].wrapping_add(temp1);
        working[3] = working[2];
        working[2] = working[1];
        working[1] = working[0];
        working[0] = temp1.wrapping_add(temp2);
    }
    for (state_word, working_word) in state.iter_mut().zip(working) {
        *state_word = state_word.wrapping_add(working_word);
    }
}
