// SPDX-License-Identifier: GPL-3.0-only
//! BME688 calibration data unpacked from NVM registers.

#[derive(Debug, Clone)]
pub struct CalibData {
    // Temperature
    pub par_t1: u16,
    pub par_t2: i16,
    pub par_t3: i8,
    // Pressure
    pub par_p1: u16,
    pub par_p2: i16,
    pub par_p3: i8,
    pub par_p4: i16,
    pub par_p5: i16,
    pub par_p6: i8,
    pub par_p7: i8,
    pub par_p8: i16,
    pub par_p9: i16,
    pub par_p10: u8,
    // Humidity
    pub par_h1: u16,
    pub par_h2: u16,
    pub par_h3: i8,
    pub par_h4: i8,
    pub par_h5: i8,
    pub par_h6: u8,
    pub par_h7: i8,
    // Gas
    pub par_g1: i8,
    pub par_g2: i16,
    pub par_g3: i8,
    pub res_heat_range: u8,
    pub res_heat_val: i8,
    pub range_sw_err: i8,
}

impl CalibData {
    /// Unpack calibration from raw NVM reads.
    /// `nvm1`: 23 bytes from registers 0x8A..0xA0
    /// `nvm2`: 14 bytes from registers 0xE1..0xEE
    /// `rhr`: register 0x02 (res_heat_range in bits [5:4])
    /// `rhv`: register 0x00 (res_heat_val)
    /// `rse`: register 0x04 (range_sw_err in bits [7:4])
    pub fn from_nvm(nvm1: &[u8], nvm2: &[u8], rhr: u8, rhv: u8, rse: u8) -> Self {
        let le16u = |a: u8, b: u8| u16::from_le_bytes([a, b]);
        let le16i = |a: u8, b: u8| i16::from_le_bytes([a, b]);

        // nvm1 offsets relative to 0x8A
        let par_t2 = le16i(nvm1[0], nvm1[1]); // 0x8A-0x8B
        let par_t3 = nvm1[2] as i8; // 0x8C
        let par_p1 = le16u(nvm1[3], nvm1[4]); // 0x8D-0x8E
        let par_p2 = le16i(nvm1[5], nvm1[6]); // 0x8F-0x90
        let par_p3 = nvm1[7] as i8; // 0x91
                                    // nvm1[8] unused                      // 0x92
        let par_p4 = le16i(nvm1[9], nvm1[10]); // 0x93-0x94
        let par_p5 = le16i(nvm1[11], nvm1[12]); // 0x95-0x96
        let par_p7 = nvm1[13] as i8; // 0x97
        let par_p6 = nvm1[14] as i8; // 0x98
                                     // nvm1[15], nvm1[16] unused           // 0x99-0x9A
        let par_p8 = le16i(nvm1[17], nvm1[18]); // 0x9B-0x9C
        let par_p9 = le16i(nvm1[19], nvm1[20]); // 0x9D-0x9E
        let par_p10 = nvm1[21]; // 0x9F
                                // nvm1[22]                             // 0xA0

        // nvm2 offsets relative to 0xE1
        let par_h2 = ((nvm2[0] as u16) << 4) | ((nvm2[1] as u16) & 0x0F); // 0xE1-0xE2
        let par_h1 = ((nvm2[2] as u16) << 4) | ((nvm2[1] as u16) >> 4); // 0xE3,0xE2
        let par_h3 = nvm2[3] as i8; // 0xE4
        let par_h4 = nvm2[4] as i8; // 0xE5
        let par_h5 = nvm2[5] as i8; // 0xE6
        let par_h6 = nvm2[6]; // 0xE7
        let par_h7 = nvm2[7] as i8; // 0xE8
        let par_t1 = le16u(nvm2[8], nvm2[9]); // 0xE9-0xEA
        let par_g2 = le16i(nvm2[10], nvm2[11]); // 0xEB-0xEC
        let par_g1 = nvm2[12] as i8; // 0xED
        let par_g3 = nvm2[13] as i8; // 0xEE

        Self {
            par_t1,
            par_t2,
            par_t3,
            par_p1,
            par_p2,
            par_p3,
            par_p4,
            par_p5,
            par_p6,
            par_p7,
            par_p8,
            par_p9,
            par_p10,
            par_h1,
            par_h2,
            par_h3,
            par_h4,
            par_h5,
            par_h6,
            par_h7,
            par_g1,
            par_g2,
            par_g3,
            res_heat_range: (rhr >> 4) & 0x03,
            res_heat_val: rhv as i8,
            range_sw_err: ((rse as i8) >> 4),
        }
    }
}
