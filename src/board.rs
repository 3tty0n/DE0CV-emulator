/// DE0-CV board state
pub struct Board {
    /// 10 red LEDs (LEDR[9:0])
    pub ledr: [bool; 10],
    /// 6 seven-segment displays (HEX0-HEX5), each 7 bits (active-low: 0=on)
    pub hex: [[bool; 7]; 6],
    /// 4 push buttons (KEY[3:0]), directly mapped
    pub key: [bool; 4],
    /// 10 DIP switches (SW[9:0])
    pub sw: [bool; 10],
    /// Reset signal (active high)
    pub rst: bool,
}

impl Board {
    pub fn new() -> Self {
        Self {
            ledr: [false; 10],
            hex: [[true; 7]; 6], // all segments off (active-low)
            key: [false; 4],
            sw: [false; 10],
            rst: false,
        }
    }

    /// Set a HEX display from a 7-bit value (active-low encoding)
    pub fn set_hex(&mut self, index: usize, value: u8) {
        for bit in 0..7 {
            self.hex[index][bit] = (value >> bit) & 1 != 0;
        }
    }

}
