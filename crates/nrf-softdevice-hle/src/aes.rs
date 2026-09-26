// SPDX-License-Identifier: MIT
//! AES-128 encryption (FIPS-197), big-endian block convention as used by the
//! Bluetooth security function e() and the nRF ECB peripheral.

const SBOX: [u8; 256] = {
    // Generated from the FIPS-197 definition at compile time.
    let mut s = [0u8; 256];
    let mut p: u8 = 1;
    let mut q: u8 = 1;
    loop {
        // p * 3
        p = p ^ (p << 1) ^ (if p & 0x80 != 0 { 0x1B } else { 0 });
        // q / 3
        q ^= q << 1;
        q ^= q << 2;
        q ^= q << 4;
        if q & 0x80 != 0 {
            q ^= 0x09;
        }
        let x = q ^ q.rotate_left(1) ^ q.rotate_left(2) ^ q.rotate_left(3) ^ q.rotate_left(4);
        s[p as usize] = x ^ 0x63;
        if p == 1 {
            break;
        }
    }
    s[0] = 0x63;
    s
};

fn xtime(x: u8) -> u8 {
    (x << 1) ^ if x & 0x80 != 0 { 0x1B } else { 0 }
}

/// AES-128 encrypt one block.
pub fn encrypt(key: &[u8; 16], block: &[u8; 16]) -> [u8; 16] {
    let mut rk = [[0u8; 16]; 11];
    rk[0] = *key;
    let mut rcon = 1u8;
    for r in 1..11 {
        let p = rk[r - 1];
        let mut t = [
            SBOX[p[13] as usize],
            SBOX[p[14] as usize],
            SBOX[p[15] as usize],
            SBOX[p[12] as usize],
        ];
        t[0] ^= rcon;
        rcon = xtime(rcon);
        let mut n = [0u8; 16];
        for i in 0..4 {
            n[i] = p[i] ^ t[i];
        }
        for i in 4..16 {
            n[i] = p[i] ^ n[i - 4];
        }
        rk[r] = n;
    }
    let mut s = *block;
    for i in 0..16 {
        s[i] ^= rk[0][i];
    }
    for r in 1..11 {
        for b in s.iter_mut() {
            *b = SBOX[*b as usize];
        }
        // ShiftRows (column-major state)
        let t = s;
        for c in 0..4 {
            for row in 0..4 {
                s[c * 4 + row] = t[((c + row) % 4) * 4 + row];
            }
        }
        if r != 10 {
            for c in 0..4 {
                let a = [s[c * 4], s[c * 4 + 1], s[c * 4 + 2], s[c * 4 + 3]];
                let all = a[0] ^ a[1] ^ a[2] ^ a[3];
                for i in 0..4 {
                    s[c * 4 + i] = a[i] ^ all ^ xtime(a[i] ^ a[(i + 1) % 4]);
                }
            }
        }
        for i in 0..16 {
            s[i] ^= rk[r][i];
        }
    }
    s
}

#[cfg(test)]
mod tests {
    #[test]
    fn fips197_appendix_b() {
        let key = [
            0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf,
            0x4f, 0x3c,
        ];
        let pt = [
            0x32, 0x43, 0xf6, 0xa8, 0x88, 0x5a, 0x30, 0x8d, 0x31, 0x31, 0x98, 0xa2, 0xe0, 0x37,
            0x07, 0x34,
        ];
        let ct = [
            0x39, 0x25, 0x84, 0x1d, 0x02, 0xdc, 0x09, 0xfb, 0xdc, 0x11, 0x85, 0x97, 0x19, 0x6a,
            0x0b, 0x32,
        ];
        assert_eq!(super::encrypt(&key, &pt), ct);
    }
}
