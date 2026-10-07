//! Placeholder place names. Each continent gets a small random "language" (a
//! subset of onsets, vowels and codas plus a few favourite endings), so names on
//! one continent sound related. Stage 3 replaces these with names from the
//! simulated cultures' phonologies.

use crate::rng::{stream, Rng};
use std::collections::HashSet;

const ONSETS: [&str; 40] = [
    "b", "c", "d", "f", "g", "h", "j", "k", "l", "m", "n", "p", "r", "s", "t", "v", "z", "br", "dr", "gr", "kr", "tr", "st", "sk",
    "th", "sh", "ch", "kh", "gh", "ph", "fl", "gl", "kl", "sl", "sv", "vr", "zh", "ts", "qu", "w",
];
const VOWELS: [&str; 16] = ["a", "e", "i", "o", "u", "a", "e", "o", "ai", "ei", "au", "ou", "ia", "y", "ae", "io"];
const CODAS: [&str; 22] = ["", "", "", "n", "r", "l", "s", "m", "th", "nd", "rn", "st", "k", "sh", "x", "rd", "lt", "ng", "z", "nt", "rk", "ss"];
const ENDINGS: [&str; 30] = [
    "a", "ia", "or", "an", "en", "ar", "is", "um", "heim", "grad", "mar", "dor", "ath", "esh", "ul", "ane", "oth", "ica", "ova", "eth",
    "ir", "ond", "ast", "and", "ur", "el", "ion", "ek", "ay", "ene",
];

#[derive(Clone, Debug)]
pub struct Lang {
    onsets: Vec<&'static str>,
    vowels: Vec<&'static str>,
    codas: Vec<&'static str>,
    endings: Vec<&'static str>,
    max_syllables: usize,
}

fn pick_subset(rng: &mut Rng, from: &[&'static str], k: usize) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = from.to_vec();
    for i in (1..v.len()).rev() {
        let j = rng.below(i + 1);
        v.swap(i, j);
    }
    v.truncate(k.min(v.len()));
    v
}

impl Lang {
    pub fn new(seed: u64, id: u64) -> Lang {
        let mut rng = Rng::new(seed ^ id.wrapping_mul(0x51_7C_C1_B7_27_22_0A_95), stream::NAMES);
        let k = [9 + rng.below(8), 4 + rng.below(4), 5 + rng.below(6), 3 + rng.below(4)];
        Lang {
            onsets: pick_subset(&mut rng, &ONSETS, k[0]),
            vowels: pick_subset(&mut rng, &VOWELS, k[1]),
            codas: pick_subset(&mut rng, &CODAS, k[2]),
            endings: pick_subset(&mut rng, &ENDINGS, k[3]),
            max_syllables: 2 + rng.below(2),
        }
    }

    pub fn word(&self, rng: &mut Rng) -> String {
        let syl = 1 + rng.below(self.max_syllables);
        let mut s = String::new();
        for k in 0..syl {
            if k > 0 || rng.f64() < 0.85 {
                s.push_str(self.onsets[rng.below(self.onsets.len())]);
            }
            s.push_str(self.vowels[rng.below(self.vowels.len())]);
            if k + 1 < syl || rng.f64() < 0.5 {
                s.push_str(self.codas[rng.below(self.codas.len())]);
            }
        }
        if syl == 1 || rng.f64() < 0.45 {
            let e = self.endings[rng.below(self.endings.len())];
            // Avoid doubled vowels at the join ("aa", "ei" + "ia").
            let last_vowel = s.chars().last().is_some_and(|c| "aeiouy".contains(c));
            let first_vowel = e.chars().next().is_some_and(|c| "aeiouy".contains(c));
            if last_vowel && first_vowel {
                s.pop();
            }
            s.push_str(e);
        }
        capitalize(&s)
    }

    /// A word not yet in `used` (falls back to a numbered word after many tries).
    pub fn unique(&self, rng: &mut Rng, used: &mut HashSet<String>) -> String {
        for _ in 0..40 {
            let w = self.word(rng);
            if w.len() >= 3 && w.len() <= 11 && pronounceable(&w) && used.insert(w.clone()) {
                return w;
            }
        }
        let w = format!("{} {}", self.word(rng), used.len());
        used.insert(w.clone());
        w
    }
}

/// No letter three times in a row, no more than three consonants or vowels in a row.
fn pronounceable(w: &str) -> bool {
    let c: Vec<char> = w.to_lowercase().chars().collect();
    let vowel = |x: char| "aeiouy".contains(x);
    let (mut run_c, mut run_v) = (0, 0);
    for (i, &x) in c.iter().enumerate() {
        if i >= 2 && c[i - 1] == x && c[i - 2] == x {
            return false;
        }
        if vowel(x) {
            run_v += 1;
            run_c = 0;
        } else {
            run_c += 1;
            run_v = 0;
        }
        if run_c > 3 || run_v > 3 {
            return false;
        }
    }
    true
}

pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Upper-case ASCII key for a name, as used in Paradox script ("STATE_VARDESH").
pub fn key(prefix: &str, name: &str) -> String {
    let body: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' }).collect();
    format!("{prefix}_{body}")
}
