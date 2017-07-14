use ::std::fmt;
use ::std::cmp::Ordering;
use ::std::ops::Sub;
use ::std::ops::Add;
use ::std::ops::Mul;
use ::std::ops::Div;

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
/// Float result of DSSIM
pub struct Dssim(f64);

impl Dssim {
    pub fn new(v: f64) -> Dssim {
        debug_assert!(v.is_finite());
        Dssim(v)
    }
}

impl fmt::Display for Dssim {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(fmt, "{:.6}", self.0)
    }
}

impl Eq for Dssim {
    // Result never has NaN
}

impl PartialEq<Dssim> for f64 {
    fn eq(&self, other: &Dssim) -> bool {
        *self == other.0
    }

    fn ne(&self, other: &Dssim) -> bool {
        *self != other.0
    }
}

impl PartialEq<f64> for Dssim {
    fn eq(&self, other: &f64) -> bool {
        self.0 == *other
    }

    fn ne(&self, other: &f64) -> bool {
        self.0 != *other
    }
}

impl From<Dssim> for f64 {
    fn from(s: Dssim) -> f64 {
        s.0
    }
}

impl From<f64> for Dssim {
    fn from(s: f64) -> Dssim {
        Dssim(s)
    }
}

impl <RHS: Into<f64>> Sub<RHS> for Dssim {
    type Output = f64;
    fn sub(self, r: RHS) -> Self::Output {
        let rval = r.into();
        debug_assert!(rval.is_finite());
        self.0.sub(rval)
    }
}


impl Sub<Dssim> for f64 {
    type Output = f64;
    fn sub(self, r: Dssim) -> Self::Output {
        self.sub(r.0)
    }
}

impl <RHS: Into<f64>> Add<RHS> for Dssim {
    type Output = f64;
    fn add(self, r: RHS) -> Self::Output {
        let rval = r.into();
        debug_assert!(rval.is_finite());
        self.0.add(rval)
    }
}

impl Add<Dssim> for f64 {
    type Output = f64;
    fn add(self, r: Dssim) -> Self::Output {
        self.add(r.0)
    }
}

impl <RHS: Into<f64>> Mul<RHS> for Dssim {
    type Output = Dssim;
    fn mul(self, r: RHS) -> Self::Output {
        let rval = r.into();
        debug_assert!(rval.is_finite());
        self.0.mul(rval).into()
    }
}

impl Mul<Dssim> for f64 {
    type Output = Dssim;
    fn mul(self, r: Dssim) -> Self::Output {
        self.mul(r.0).into()
    }
}

impl <RHS: Into<f64>> Div<RHS> for Dssim {
    type Output = f64;
    fn div(self, r: RHS) -> Self::Output {
        let rval = r.into();
        debug_assert!(rval.is_finite() && rval != 0.);
        self.0.div(rval)
    }
}

impl Div<Dssim> for f64 {
    type Output = f64;
    fn div(self, r: Dssim) -> Self::Output {
        debug_assert!(r.0 != 0.);
        self.div(r.0)
    }
}

impl PartialOrd<f64> for Dssim {
    fn partial_cmp(&self, other: &f64) -> Option<Ordering> {
        self.0.partial_cmp(other)
    }

    fn lt(&self, other: &f64) -> bool { self.0.lt(other) }
    fn le(&self, other: &f64) -> bool { self.0.le(other) }
    fn gt(&self, other: &f64) -> bool { self.0.gt(other) }
    fn ge(&self, other: &f64) -> bool { self.0.ge(other) }
}

impl PartialOrd<Dssim> for f64 {
    fn partial_cmp(&self, other: &Dssim) -> Option<Ordering> {
        self.partial_cmp(&other.0)
    }

    fn lt(&self, other: &Dssim) -> bool { self.lt(&other.0) }
    fn le(&self, other: &Dssim) -> bool { self.le(&other.0) }
    fn gt(&self, other: &Dssim) -> bool { self.gt(&other.0) }
    fn ge(&self, other: &Dssim) -> bool { self.ge(&other.0) }
}
