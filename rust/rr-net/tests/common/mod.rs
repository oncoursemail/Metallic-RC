//! A simulated one-way link that drops and reorders datagrams, deterministic so a failure replays.
#![allow(dead_code)]

pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub fn chance(&mut self, p: f64) -> bool {
        (self.next() % 10_000) as f64 / 10_000.0 < p
    }
    pub fn up_to(&mut self, max: f64) -> f64 {
        (self.next() % 10_000) as f64 / 10_000.0 * max
    }
}

/// Datagrams in flight one way: (arrival time, bytes).
pub struct Link {
    rng: Rng,
    pub drop: f64,
    max_delay: f64,
    queue: Vec<(f64, Vec<u8>)>,
    pub sent: usize,
}

impl Link {
    pub fn new(seed: u64, drop: f64, max_delay: f64) -> Self {
        Link { rng: Rng(seed), drop, max_delay, queue: vec![], sent: 0 }
    }
    /// `datagram` is already wrapped by the RR layer.
    pub fn send(&mut self, now: f64, datagram: Vec<u8>) {
        self.sent += 1;
        if !self.rng.chance(self.drop) {
            // a base delay plus jitter: later datagrams often overtake earlier ones
            let at = now + 0.01 + self.rng.up_to(self.max_delay);
            self.queue.push((at, datagram));
        }
    }
    pub fn arrived(&mut self, now: f64) -> Vec<Vec<u8>> {
        let (ready, wait): (Vec<_>, Vec<_>) = self.queue.drain(..).partition(|(t, _)| *t <= now);
        self.queue = wait;
        ready.into_iter().map(|(_, d)| d).collect()
    }
}
