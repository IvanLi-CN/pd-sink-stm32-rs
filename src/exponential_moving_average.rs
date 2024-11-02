pub(crate) struct ExponentialMovingAverage {
    alpha: f64,
    average: f64,
}

impl ExponentialMovingAverage {
    pub fn new(alpha: f64) -> Self {
        ExponentialMovingAverage {
            alpha,
            average: 0.0,
        }
    }

    pub fn update(&mut self, value: f64) -> f64 {
        self.average = self.alpha * value + (1.0 - self.alpha) * self.average;
        self.average
    }

    pub fn get_average(&self) -> f64 {
        self.average
    }
}
