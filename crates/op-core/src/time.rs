//! Exact time.
//!
//! Every persistent position and duration is an integer number of ticks. One second is
//! 705,600,000 ticks: that number is a multiple of every supported frame duration (including the
//! 1000/1001 NTSC family) and of every supported audio sample period, so frame and sample
//! boundaries are always whole tick counts and edit arithmetic never rounds (DM-TIME-001). The
//! proof is the `supported_rates_are_exact` test and docs/adr/0002-time-model.md.
//!
//! Sequence time and source (media) time are different domains (DM-TIME-003): a `Time<Seq>` cannot
//! be added to or compared with a `Time<Src>`. Durations (`Dur`) are domain free; crossing domains
//! goes through a clip's time transform. Intervals are half open, `[start, end)` (DM-TIME-002).

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Ticks in one second.
pub const TICKS_PER_SECOND: i64 = 705_600_000;

/// Sequence (timeline) time domain.
#[derive(Debug)]
pub enum Seq {}
/// Source media time domain; effect keyframes also live here, so they stay attached to the content.
#[derive(Debug)]
pub enum Src {}

/// A position in time domain `D`.
pub struct Time<D> {
    ticks: i64,
    _domain: PhantomData<fn() -> D>,
}

pub type SeqTime = Time<Seq>;
pub type SrcTime = Time<Src>;

impl<D> Time<D> {
    pub const ZERO: Self = Self::from_ticks(0);
    pub const MAX: Self = Self::from_ticks(i64::MAX / 4);

    pub const fn from_ticks(ticks: i64) -> Self {
        Self {
            ticks,
            _domain: PhantomData,
        }
    }

    pub const fn ticks(self) -> i64 {
        self.ticks
    }

    pub fn from_seconds(seconds: f64) -> Self {
        Self::from_ticks((seconds * TICKS_PER_SECOND as f64).round() as i64)
    }

    pub fn seconds(self) -> f64 {
        self.ticks as f64 / TICKS_PER_SECOND as f64
    }

    /// Offset from zero as a duration.
    pub const fn since_zero(self) -> Dur {
        Dur(self.ticks)
    }

    pub fn min(self, other: Self) -> Self {
        if self.ticks <= other.ticks {
            self
        } else {
            other
        }
    }

    pub fn max(self, other: Self) -> Self {
        if self.ticks >= other.ticks {
            self
        } else {
            other
        }
    }

    pub fn clamp(self, lo: Self, hi: Self) -> Self {
        self.max(lo).min(hi)
    }

    /// The start of the frame that contains this time.
    pub fn floor_frame(self, rate: Rate) -> Self {
        rate.frame_start(rate.frame_of(self))
    }

    /// The nearest frame boundary.
    pub fn round_frame(self, rate: Rate) -> Self {
        let f = rate.frame_of(self + rate.frame_duration() / 2);
        rate.frame_start(f)
    }

    /// Reinterprets the tick count in another domain. Only for adapters and mapping code that has
    /// established the relation between the two domains.
    pub const fn cast<E>(self) -> Time<E> {
        Time::from_ticks(self.ticks)
    }
}

impl<D> Clone for Time<D> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<D> Copy for Time<D> {}
impl<D> PartialEq for Time<D> {
    fn eq(&self, other: &Self) -> bool {
        self.ticks == other.ticks
    }
}
impl<D> Eq for Time<D> {}
impl<D> PartialOrd for Time<D> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<D> Ord for Time<D> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.ticks.cmp(&other.ticks)
    }
}
impl<D> Hash for Time<D> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ticks.hash(state);
    }
}
impl<D> Default for Time<D> {
    fn default() -> Self {
        Self::ZERO
    }
}
impl<D> fmt::Debug for Time<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}t({:.4}s)", self.ticks, self.seconds())
    }
}
impl<D> Serialize for Time<D> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i64(self.ticks)
    }
}
impl<'de, D> Deserialize<'de> for Time<D> {
    fn deserialize<De: Deserializer<'de>>(d: De) -> Result<Self, De::Error> {
        i64::deserialize(d).map(Self::from_ticks)
    }
}

impl<D> Add<Dur> for Time<D> {
    type Output = Self;
    fn add(self, rhs: Dur) -> Self {
        Self::from_ticks(self.ticks + rhs.0)
    }
}
impl<D> AddAssign<Dur> for Time<D> {
    fn add_assign(&mut self, rhs: Dur) {
        self.ticks += rhs.0;
    }
}
impl<D> Sub<Dur> for Time<D> {
    type Output = Self;
    fn sub(self, rhs: Dur) -> Self {
        Self::from_ticks(self.ticks - rhs.0)
    }
}
impl<D> SubAssign<Dur> for Time<D> {
    fn sub_assign(&mut self, rhs: Dur) {
        self.ticks -= rhs.0;
    }
}
impl<D> Sub for Time<D> {
    type Output = Dur;
    fn sub(self, rhs: Self) -> Dur {
        Dur(self.ticks - rhs.ticks)
    }
}

/// A signed length of time.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Dur(pub i64);

impl Dur {
    pub const ZERO: Dur = Dur(0);

    pub fn from_seconds(seconds: f64) -> Self {
        Dur((seconds * TICKS_PER_SECOND as f64).round() as i64)
    }

    pub fn seconds(self) -> f64 {
        self.0 as f64 / TICKS_PER_SECOND as f64
    }

    pub fn ticks(self) -> i64 {
        self.0
    }

    pub fn abs(self) -> Self {
        Dur(self.0.abs())
    }

    pub fn is_positive(self) -> bool {
        self.0 > 0
    }

    pub fn is_negative(self) -> bool {
        self.0 < 0
    }

    pub fn max(self, other: Self) -> Self {
        if self >= other { self } else { other }
    }

    pub fn min(self, other: Self) -> Self {
        if self <= other { self } else { other }
    }

    pub fn clamp(self, lo: Self, hi: Self) -> Self {
        self.max(lo).min(hi)
    }

    /// Rounds to a whole number of frames.
    pub fn round_frames(self, rate: Rate) -> Self {
        let fd = rate.frame_duration().0;
        if fd == 0 {
            return self;
        }
        Dur(div_round(self.0 as i128, fd as i128) as i64 * fd)
    }
}

impl fmt::Debug for Dur {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}t({:.4}s)", self.0, self.seconds())
    }
}
impl Add for Dur {
    type Output = Dur;
    fn add(self, rhs: Dur) -> Dur {
        Dur(self.0 + rhs.0)
    }
}
impl AddAssign for Dur {
    fn add_assign(&mut self, rhs: Dur) {
        self.0 += rhs.0;
    }
}
impl Sub for Dur {
    type Output = Dur;
    fn sub(self, rhs: Dur) -> Dur {
        Dur(self.0 - rhs.0)
    }
}
impl SubAssign for Dur {
    fn sub_assign(&mut self, rhs: Dur) {
        self.0 -= rhs.0;
    }
}
impl Neg for Dur {
    type Output = Dur;
    fn neg(self) -> Dur {
        Dur(-self.0)
    }
}
impl Mul<i64> for Dur {
    type Output = Dur;
    fn mul(self, rhs: i64) -> Dur {
        Dur(self.0 * rhs)
    }
}
impl Div<i64> for Dur {
    type Output = Dur;
    fn div(self, rhs: i64) -> Dur {
        Dur(self.0 / rhs)
    }
}

/// Rounds `n / d` to the nearest integer, halves away from zero.
pub fn div_round(n: i128, d: i128) -> i128 {
    debug_assert!(d != 0);
    let (n, d) = if d < 0 { (-n, -d) } else { (n, d) };
    if n >= 0 {
        (n + d / 2) / d
    } else {
        -((-n + d / 2) / d)
    }
}

/// Floor division for signed values.
pub fn div_floor(n: i128, d: i128) -> i128 {
    let q = n / d;
    if (n % d != 0) && ((n < 0) != (d < 0)) {
        q - 1
    } else {
        q
    }
}

/// A frame or sample rate as a rational number of units per second.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rate {
    pub num: u32,
    pub den: u32,
}

impl fmt::Debug for Rate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.num, self.den)
    }
}

impl Rate {
    pub const fn new(num: u32, den: u32) -> Self {
        Rate { num, den }
    }
    pub const fn fps(n: u32) -> Self {
        Rate { num: n, den: 1 }
    }

    pub const FPS_23_976: Rate = Rate::new(24000, 1001);
    pub const FPS_24: Rate = Rate::fps(24);
    pub const FPS_25: Rate = Rate::fps(25);
    pub const FPS_29_97: Rate = Rate::new(30000, 1001);
    pub const FPS_30: Rate = Rate::fps(30);
    pub const FPS_50: Rate = Rate::fps(50);
    pub const FPS_59_94: Rate = Rate::new(60000, 1001);
    pub const FPS_60: Rate = Rate::fps(60);
    pub const HZ_44100: Rate = Rate::fps(44100);
    pub const HZ_48000: Rate = Rate::fps(48000);

    /// Frame rates offered for sequences.
    pub const SEQUENCE_RATES: [Rate; 17] = [
        Rate::fps(10),
        Rate::fps(12),
        Rate::new(25, 2),
        Rate::fps(15),
        Rate::FPS_23_976,
        Rate::FPS_24,
        Rate::FPS_25,
        Rate::FPS_29_97,
        Rate::FPS_30,
        Rate::new(48000, 1001),
        Rate::fps(48),
        Rate::FPS_50,
        Rate::FPS_59_94,
        Rate::FPS_60,
        Rate::fps(100),
        Rate::new(120000, 1001),
        Rate::fps(120),
    ];

    /// Audio sample rates offered for sequences and export.
    pub const SAMPLE_RATES: [Rate; 12] = [
        Rate::fps(8000),
        Rate::fps(11025),
        Rate::fps(16000),
        Rate::fps(22050),
        Rate::fps(24000),
        Rate::fps(32000),
        Rate::HZ_44100,
        Rate::HZ_48000,
        Rate::fps(88200),
        Rate::fps(96000),
        Rate::fps(176400),
        Rate::fps(192000),
    ];

    pub fn as_f64(self) -> f64 {
        self.num as f64 / self.den.max(1) as f64
    }

    pub fn is_valid(self) -> bool {
        self.num > 0 && self.den > 0
    }

    /// Whether every unit boundary of this rate is a whole number of ticks.
    pub fn is_exact(self) -> bool {
        self.is_valid() && (TICKS_PER_SECOND as i128 * self.den as i128) % self.num as i128 == 0
    }

    /// Nominal integer rate used for timecode (30 for 29.97).
    pub fn timecode_base(self) -> u32 {
        (self.as_f64().round() as u32).max(1)
    }

    /// Whether drop-frame timecode applies (29.97 and 59.94 families).
    pub fn supports_drop_frame(self) -> bool {
        self.den == 1001 && (self.num == 30000 || self.num == 60000)
    }

    /// Length of one unit (frame or sample). Exact for exact rates.
    pub fn frame_duration(self) -> Dur {
        if !self.is_valid() {
            return Dur(0);
        }
        Dur(div_round(
            TICKS_PER_SECOND as i128 * self.den as i128,
            self.num as i128,
        ) as i64)
    }

    /// Start time of unit `n` (frame or sample index).
    pub fn frame_start<D>(self, n: i64) -> Time<D> {
        Time::from_ticks(self.frames_to_dur(n).0)
    }

    /// Duration of `n` units.
    pub fn frames_to_dur(self, n: i64) -> Dur {
        if !self.is_valid() {
            return Dur(0);
        }
        Dur(div_round(
            n as i128 * TICKS_PER_SECOND as i128 * self.den as i128,
            self.num as i128,
        ) as i64)
    }

    /// Index of the unit that contains `t` (floor).
    pub fn frame_of<D>(self, t: Time<D>) -> i64 {
        self.dur_to_frames_floor(t.since_zero())
    }

    pub fn dur_to_frames_floor(self, d: Dur) -> i64 {
        if !self.is_valid() {
            return 0;
        }
        div_floor(
            d.0 as i128 * self.num as i128,
            TICKS_PER_SECOND as i128 * self.den as i128,
        ) as i64
    }

    pub fn dur_to_frames_round(self, d: Dur) -> i64 {
        if !self.is_valid() {
            return 0;
        }
        div_round(
            d.0 as i128 * self.num as i128,
            TICKS_PER_SECOND as i128 * self.den as i128,
        ) as i64
    }

    pub fn dur_to_frames_ceil(self, d: Dur) -> i64 {
        -Rate::dur_to_frames_floor(self, -d)
    }

    /// A short human label: "23.976", "25", "29.97".
    pub fn label(self) -> String {
        if self.den == 1 {
            format!("{}", self.num)
        } else {
            let v = self.as_f64();
            let s = format!("{v:.3}");
            let s = s.trim_end_matches('0').trim_end_matches('.');
            s.to_string()
        }
    }

    /// Best rational approximation for a floating point rate from a media file.
    pub fn from_f64(v: f64) -> Rate {
        if !(v.is_finite() && v > 0.0) {
            return Rate::FPS_25;
        }
        for r in Rate::SEQUENCE_RATES {
            if (r.as_f64() - v).abs() < 0.002 {
                return r;
            }
        }
        if (v - v.round()).abs() < 1e-6 {
            return Rate::fps(v.round() as u32);
        }
        Rate::new((v * 1000.0).round() as u32, 1000)
    }
}

/// A positive rational speed factor: source duration per sequence duration.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Speed {
    pub num: i64,
    pub den: i64,
}

impl fmt::Debug for Speed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3}%", self.percent())
    }
}

impl Default for Speed {
    fn default() -> Self {
        Speed::NORMAL
    }
}

fn gcd(mut a: i64, mut b: i64) -> i64 {
    a = a.abs();
    b = b.abs();
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a.max(1)
}

impl Speed {
    pub const NORMAL: Speed = Speed { num: 1, den: 1 };

    /// A reduced ratio. Both parts must be positive.
    pub fn new(num: i64, den: i64) -> Speed {
        assert!(num > 0 && den > 0, "speed must be positive");
        let g = gcd(num, den);
        Speed {
            num: num / g,
            den: den / g,
        }
    }

    /// Speed that maps a sequence duration onto a source duration exactly.
    pub fn from_durations(source: Dur, sequence: Dur) -> Speed {
        Speed::new(source.0.max(1), sequence.0.max(1))
    }

    /// Speed from a percentage (100 = normal) with 1/100000 precision.
    pub fn from_percent(p: f64) -> Speed {
        let p = p.clamp(0.001, 1_000_000.0);
        Speed::new((p * 1000.0).round() as i64, 100_000)
    }

    pub fn percent(self) -> f64 {
        self.num as f64 * 100.0 / self.den as f64
    }

    pub fn as_f64(self) -> f64 {
        self.num as f64 / self.den as f64
    }

    pub fn is_normal(self) -> bool {
        self.num == self.den
    }

    /// Sequence duration -> source duration.
    pub fn to_source(self, d: Dur) -> Dur {
        Dur(div_round(d.0 as i128 * self.num as i128, self.den as i128) as i64)
    }

    /// Source duration -> sequence duration.
    pub fn to_sequence(self, d: Dur) -> Dur {
        Dur(div_round(d.0 as i128 * self.den as i128, self.num as i128) as i64)
    }
}

/// A half-open interval `[start, end)` in domain `D`.
pub struct Range<D> {
    pub start: Time<D>,
    pub end: Time<D>,
}

pub type SeqRange = Range<Seq>;
pub type SrcRange = Range<Src>;

impl<D> Range<D> {
    pub fn new(start: Time<D>, end: Time<D>) -> Self {
        Range { start, end }
    }

    pub fn with_duration(start: Time<D>, dur: Dur) -> Self {
        Range {
            start,
            end: start + dur,
        }
    }

    pub fn duration(&self) -> Dur {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    pub fn contains(&self, t: Time<D>) -> bool {
        t >= self.start && t < self.end
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        self.start < other.end && other.start < self.end
    }

    pub fn intersect(&self, other: &Self) -> Option<Self> {
        let s = self.start.max(other.start);
        let e = self.end.min(other.end);
        (s < e).then(|| Range::new(s, e))
    }

    pub fn shifted(&self, d: Dur) -> Self {
        Range::new(self.start + d, self.end + d)
    }
}

impl<D> Clone for Range<D> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<D> Copy for Range<D> {}
impl<D> PartialEq for Range<D> {
    fn eq(&self, o: &Self) -> bool {
        self.start == o.start && self.end == o.end
    }
}
impl<D> Eq for Range<D> {}
impl<D> Hash for Range<D> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.start.hash(state);
        self.end.hash(state);
    }
}
impl<D> fmt::Debug for Range<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}, {:?})", self.start, self.end)
    }
}
impl<D> Serialize for Range<D> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        (self.start.ticks, self.end.ticks).serialize(s)
    }
}
impl<'de, D> Deserialize<'de> for Range<D> {
    fn deserialize<De: Deserializer<'de>>(d: De) -> Result<Self, De::Error> {
        let (a, b) = <(i64, i64)>::deserialize(d)?;
        Ok(Range::new(Time::from_ticks(a), Time::from_ticks(b)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_rates_are_exact() {
        for r in Rate::SEQUENCE_RATES.iter().chain(Rate::SAMPLE_RATES.iter()) {
            assert!(r.is_exact(), "{r:?} is not exact");
            // frame n starts at an exact multiple, and converting back yields n
            for n in [0i64, 1, 2, 999, 1001, 86_400 * 60] {
                let t: SeqTime = r.frame_start(n);
                assert_eq!(r.frame_of(t), n, "{r:?} frame {n}");
                assert_eq!(r.frames_to_dur(n).0, n * r.frame_duration().0);
            }
        }
    }

    #[test]
    fn a_day_at_the_finest_rate_fits_with_headroom() {
        let day = Rate::fps(192_000).frames_to_dur(192_000 * 86_400);
        assert!(day.0.checked_mul(1000).is_some());
    }

    #[test]
    fn domains_do_not_mix_but_durations_cross() {
        let a = SeqTime::from_seconds(2.0);
        let b = SeqTime::from_seconds(0.5);
        assert_eq!((a - b).seconds(), 1.5);
        let s = SrcTime::ZERO + (a - b);
        assert_eq!(s.seconds(), 1.5);
    }

    #[test]
    fn frame_floor_and_round() {
        let r = Rate::FPS_29_97;
        let t = r.frame_start::<Seq>(10) + Dur(1);
        assert_eq!(t.floor_frame(r), r.frame_start(10));
        let t2 = r.frame_start::<Seq>(10) + r.frame_duration() * 3 / 4;
        assert_eq!(t2.round_frame(r), r.frame_start(11));
        assert_eq!(r.frame_of(SeqTime::from_ticks(-1)), -1);
    }

    #[test]
    fn speed_maps_exactly() {
        let s = Speed::from_durations(Dur::from_seconds(10.0), Dur::from_seconds(5.0));
        assert_eq!(s, Speed::new(2, 1));
        assert_eq!(s.to_source(Dur::from_seconds(1.0)), Dur::from_seconds(2.0));
        assert_eq!(
            s.to_sequence(Dur::from_seconds(2.0)),
            Dur::from_seconds(1.0)
        );
        assert!((Speed::from_percent(33.333).percent() - 33.333).abs() < 1e-9);
    }

    #[test]
    fn ranges() {
        let r = SeqRange::new(SeqTime::from_ticks(10), SeqTime::from_ticks(20));
        assert!(r.contains(SeqTime::from_ticks(10)));
        assert!(!r.contains(SeqTime::from_ticks(20)));
        let o = SeqRange::new(SeqTime::from_ticks(20), SeqTime::from_ticks(30));
        assert!(!r.overlaps(&o));
        assert_eq!(r.intersect(&o), None);
    }

    #[test]
    fn rate_labels() {
        assert_eq!(Rate::FPS_23_976.label(), "23.976");
        assert_eq!(Rate::FPS_29_97.label(), "29.97");
        assert_eq!(Rate::FPS_25.label(), "25");
        assert_eq!(Rate::from_f64(29.97002997), Rate::FPS_29_97);
    }
}
