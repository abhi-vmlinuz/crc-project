//! Textbook CRC state: data bits + generator bits, nothing else.

/// Which panel has keyboard focus.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Pane {
    Inputs,
    Division,
    Receiver,
}

impl Pane {
    pub const ORDER: [Pane; 3] = [Pane::Inputs, Pane::Division, Pane::Receiver];

    pub fn title(self) -> &'static str {
        match self {
            Self::Inputs => "1 INPUTS + TRANSMITTED FRAME",
            Self::Division => "2 DIVISION (SENDER SIDE)",
            Self::Receiver => "3 CHANNEL + RECEIVER",
        }
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Screen {
    Lab,
    Help,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum EditTarget {
    Data,
    Generator,
}

/// One division step for display.
#[derive(Clone, Debug)]
pub struct Step {
    pub index: usize,
    pub xored: bool,
    pub bits: Vec<u8>,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum DataMode {
    Text,
    Bits,
}

pub struct App {
    pub screen: Screen,
    pub focus: Pane,
    pub data_mode: DataMode,
    pub data_text: String,
    pub generator_text: String,
    pub data: Vec<u8>,
    pub generator: Vec<u8>,
    pub frame: Vec<u8>,
    pub crc: Vec<u8>,
    pub received: Vec<u8>,
    pub damaged: Vec<usize>,
    pub steps: Vec<Step>,
    pub augmented: Vec<u8>,
    pub step: usize,
    pub playing: bool,
    /// Manual scroll-back in the division history (0 = follow cursor).
    pub div_scroll: usize,
    pub editing: Option<EditTarget>,
    pub cursor: usize,
    pub error: Option<String>,
    pub should_quit: bool,
    pub theme_index: usize,
}

impl Default for App {
    fn default() -> Self {
        let mut app = Self {
            screen: Screen::Lab,
            focus: Pane::Inputs,
            data_mode: DataMode::Text,
            data_text: "HELLO".to_string(),
            generator_text: "1011".to_string(),
            data: Vec::new(),
            generator: Vec::new(),
            frame: Vec::new(),
            crc: Vec::new(),
            received: Vec::new(),
            damaged: Vec::new(),
            steps: Vec::new(),
            augmented: Vec::new(),
            step: 0,
            playing: false,
            div_scroll: 0,
            editing: None,
            cursor: 0,
            error: None,
            should_quit: false,
            theme_index: 0,
        };
        app.rebuild();
        app
    }
}

/// Keep only 0/1.
pub fn parse_bits(text: &str) -> Vec<u8> {
    text.chars()
        .filter_map(|c| match c {
            '0' => Some(0),
            '1' => Some(1),
            _ => None,
        })
        .collect()
}

pub fn valid_bits(text: &str, min_len: usize, max_len: usize) -> bool {
    let t = text.trim();
    if t.len() < min_len || t.len() > max_len {
        return false;
    }
    t.chars().all(|c| c == '0' || c == '1')
}

/// Expand bytes to MSB-first bits.
pub fn bytes_to_bits(bytes: &[u8]) -> Vec<u8> {
    let mut bits = Vec::with_capacity(bytes.len() * 8);
    for &b in bytes {
        for i in (0..8).rev() {
            bits.push((b >> i) & 1);
        }
    }
    bits
}

impl App {
    /// Recompute frame + division from the two text fields.
    pub fn rebuild(&mut self) {
        self.error = None;
        self.playing = false;
        self.div_scroll = 0;
        let data = match self.data_mode {
            DataMode::Bits => {
                if !valid_bits(&self.data_text, 1, 64) {
                    self.error = Some("Data bits: type only 0 and 1 (1-64 bits).".to_string());
                    self.clear_computed();
                    return;
                }
                parse_bits(&self.data_text)
            }
            DataMode::Text => {
                if self.data_text.is_empty() || self.data_text.len() > 12 {
                    self.error = Some("Message: type 1-12 characters.".to_string());
                    self.clear_computed();
                    return;
                }
                bytes_to_bits(self.data_text.as_bytes())
            }
        };
        if !valid_bits(&self.generator_text, 2, 17) {
            self.error = Some("Generator must be 2-17 bits of 0/1.".to_string());
            self.clear_computed();
            return;
        }
        let generator = parse_bits(&self.generator_text);
        if generator.first() != Some(&1) || generator.last() != Some(&1) {
            self.error = Some("Generator must start and end with 1 (e.g. 1011).".to_string());
            self.clear_computed();
            return;
        }
        let n = generator.len();
        let mut work = data.clone();
        work.extend(std::iter::repeat_n(0, n - 1));
        let augmented = work.clone();
        let mut steps = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            let mut xored = false;
            if work[i] == 1 {
                for (j, &g) in generator.iter().enumerate() {
                    work[i + j] ^= g;
                }
                xored = true;
            }
            steps.push(Step {
                index: i,
                xored,
                bits: work.clone(),
            });
        }
        let crc = work[data.len()..].to_vec();
        let mut frame = data.clone();
        frame.extend(crc.iter().copied());

        self.data = data;
        self.generator = generator;
        self.crc = crc;
        self.frame = frame.clone();
        self.received = frame;
        self.damaged.clear();
        self.steps = steps;
        self.augmented = augmented;
        self.step = 0;
    }

    fn clear_computed(&mut self) {
        self.data.clear();
        self.generator.clear();
        self.frame.clear();
        self.crc.clear();
        self.received.clear();
        self.damaged.clear();
        self.steps.clear();
        self.augmented.clear();
        self.step = 0;
    }

    /// Receiver check: divide received frame by G. All-zero remainder = valid.
    pub fn remainder(&self) -> Vec<u8> {
        if self.generator.is_empty() || self.received.is_empty() {
            return Vec::new();
        }
        let n = self.generator.len();
        if self.received.len() < n {
            return self.received.clone();
        }
        let mut work = self.received.clone();
        let limit = work.len() - n + 1;
        for i in 0..limit {
            if work[i] == 1 {
                for (j, &g) in self.generator.iter().enumerate() {
                    work[i + j] ^= g;
                }
            }
        }
        work[work.len() - (n - 1)..].to_vec()
    }

    pub fn is_valid(&self) -> bool {
        let r = self.remainder();
        !r.is_empty() && r.iter().all(|&b| b == 0)
    }

    pub fn flip(&mut self, pos: usize) {
        if pos >= self.received.len() {
            return;
        }
        self.received[pos] ^= 1;
        if self.received[pos] == self.frame[pos] {
            self.damaged.retain(|&p| p != pos);
        } else if !self.damaged.contains(&pos) {
            self.damaged.push(pos);
            self.damaged.sort_unstable();
        }
    }

    pub fn inject_random(&mut self) {
        if self.received.is_empty() {
            return;
        }
        let mut seed =
            self.damaged.len() as u64 * 0x9E37_79B9_7F4A_7C15 + self.received.len() as u64 + 1;
        seed ^= seed >> 12;
        seed ^= seed << 25;
        seed ^= seed >> 27;
        let pos = (seed.wrapping_mul(0x2545_F491_4F6C_DD1D) % self.received.len() as u64) as usize;
        self.flip(pos);
    }

    pub fn clear_errors(&mut self) {
        self.received = self.frame.clone();
        self.damaged.clear();
    }

    pub fn enter_editing(&mut self, target: EditTarget) {
        self.editing = Some(target);
        self.cursor = match target {
            EditTarget::Data => self.data_text.len(),
            EditTarget::Generator => self.generator_text.len(),
        };
    }

    pub fn exit_editing(&mut self) {
        self.editing = None;
        self.rebuild();
    }

    pub fn tick(&mut self) {
        if !self.playing || self.steps.is_empty() {
            return;
        }
        // Stop at the last step instead of wrapping: looping back to 1/32
        // mid-demo looks like a glitch and confuses the audience.
        if self.step + 1 >= self.steps.len() {
            self.playing = false;
        } else {
            self.step += 1;
        }
    }
}

/// Format bits as a plain string.
pub fn bits_to_string(bits: &[u8]) -> String {
    bits.iter().map(|b| if *b == 1 { '1' } else { '0' }).collect()
}
