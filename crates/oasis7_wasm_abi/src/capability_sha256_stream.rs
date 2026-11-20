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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_sha_padding_and_chunk_boundaries_match_independent_vectors() {
        // Fixed SHA-256 vectors generated independently with Python hashlib.
        let vectors = [
            (
                0,
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                1,
                "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb",
            ),
            (
                55,
                "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318",
            ),
            (
                56,
                "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a",
            ),
            (
                63,
                "7d3e74a05d7db15bce4ad9ec0658ea98e3f06eeecf16b4c6fff2da457ddc2f34",
            ),
            (
                64,
                "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb",
            ),
            (
                65,
                "635361c48bb9eab14198e76ea8ab7f1a41685d6ad62aa9146d301d4f17eb0ae0",
            ),
            (
                119,
                "31eba51c313a5c08226adf18d4a359cfdfd8d2e816b13f4af952f7ea6584dcfb",
            ),
            (
                120,
                "2f3d335432c70b580af0e8e1b3674a7c020d683aa5f73aaaedfdc55af904c21c",
            ),
            (
                127,
                "c57e9278af78fa3cab38667bef4ce29d783787a2f731d4e12200270f0c32320a",
            ),
            (
                128,
                "6836cf13bac400e9105071cd6af47084dfacad4e5e302c94bfed24e013afb73e",
            ),
            (
                8193,
                "9c10c48d1f1d6618db88fde2c25409181c9201ed34ec6815d62bcf57c10d177b",
            ),
        ];
        for (length, expected) in vectors {
            let bytes = vec![b'a'; length];
            assert_eq!(
                super::super::canonical_sha256_hex(&bytes).unwrap(),
                expected
            );
            for chunk_size in [1, 7, 63, 64, 65, 8192] {
                let mut hash = Sha256V1::new();
                for chunk in bytes.chunks(chunk_size) {
                    hash.update(chunk, &mut |_, _| Ok(())).unwrap();
                }
                let digest = hash.finish(&mut |_, _| Ok(())).unwrap();
                assert_eq!(super::super::hex_lower(&digest), expected);
            }
        }
    }
}
