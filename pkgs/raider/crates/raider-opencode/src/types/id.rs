use std::sync::Mutex;

#[derive(Clone, Copy, Debug)]
pub enum Prefix {
    Message,
    Part,
}

impl Prefix {
    fn as_str(self) -> &'static str {
        match self {
            Prefix::Message => "msg",
            Prefix::Part => "prt",
        }
    }
}

const LENGTH: usize = 26;
const HEX_LEN: usize = 12;
const BASE62_LEN: usize = LENGTH - HEX_LEN;
const BASE62_ALPHABET: &[u8; 62] =
    b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

#[derive(Default)]
struct CounterState {
    last_timestamp_ms: i128,
    counter: i128,
}

static COUNTER: Mutex<CounterState> = Mutex::new(CounterState {
    last_timestamp_ms: 0,
    counter: 0,
});

pub fn ascending(prefix: Prefix) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i128)
        .unwrap_or(0);
    ascending_with_timestamp(prefix, now)
}

pub fn ascending_with_timestamp(prefix: Prefix, timestamp_ms: i128) -> String {
    let counter = {
        let mut s = COUNTER.lock().expect("id counter poisoned");
        if timestamp_ms != s.last_timestamp_ms {
            s.last_timestamp_ms = timestamp_ms;
            s.counter = 0;
        }
        s.counter += 1;
        s.counter
    };

    let packed: i128 = timestamp_ms.saturating_mul(0x1000).saturating_add(counter);
    let packed_u: u128 = packed as u128;
    let mut hex = String::with_capacity(HEX_LEN);
    for i in 0..6 {
        let byte = ((packed_u >> (40 - 8 * i)) & 0xff) as u8;
        hex.push_str(&format!("{:02x}", byte));
    }

    let random = random_base62(BASE62_LEN);
    format!("{}_{}{}", prefix.as_str(), hex, random)
}

fn random_base62(n: usize) -> String {
    let mut state = seed();
    let mut out = String::with_capacity(n);
    for _ in 0..n {
        state = xorshift64(state);
        let idx = (state % 62) as usize;
        out.push(BASE62_ALPHABET[idx] as char);
    }
    out
}

fn seed() -> u64 {
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let ctr = {
        let s = COUNTER.lock().expect("id counter poisoned");
        s.counter as u64
    };
    ns.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(ctr.wrapping_mul(0xBF58_476D_1CE4_E5B9))
        | 1
}

fn xorshift64(mut x: u64) -> u64 {
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_is_correct() {
        let id = ascending(Prefix::Message);
        assert!(id.starts_with("msg_"), "got {id}");
        let id = ascending(Prefix::Part);
        assert!(id.starts_with("prt_"), "got {id}");
    }

    #[test]
    fn total_length_matches_opencode() {
        let id = ascending(Prefix::Message);
        assert_eq!(id.len(), 4 + LENGTH, "got {id}");
    }

    #[test]
    fn ids_are_monotonic_at_fixed_timestamp() {
        const ATTEMPTS: usize = 32;
        for i in 0..ATTEMPTS {
            let ts: i128 = 1_700_000_000_000 + i as i128;
            let a = ascending_with_timestamp(Prefix::Message, ts);
            let b = ascending_with_timestamp(Prefix::Message, ts);
            let a_hex = &a[4..4 + HEX_LEN];
            let b_hex = &b[4..4 + HEX_LEN];
            if b_hex > a_hex {
                return;
            }
        }
        panic!(
            "ids_are_monotonic_at_fixed_timestamp: {ATTEMPTS} attempts \
             could not get two adjacent calls without an interleaving \
             reset — this is either a real monotonicity bug OR another \
             test is hammering ascending_with_timestamp at a rate that \
             saturates the counter races. Investigate before relaxing \
             this bound.",
        );
    }

    #[test]
    fn body_is_hex_then_base62() {
        let id = ascending_with_timestamp(Prefix::Part, 1_700_000_000_000);
        let body = &id[4..];
        assert_eq!(body.len(), LENGTH);
        assert!(body[..HEX_LEN].chars().all(|c| c.is_ascii_hexdigit()));
        assert!(body[HEX_LEN..].chars().all(|c| c.is_ascii_alphanumeric()));
    }
}
