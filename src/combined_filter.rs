use core::cmp::Ordering;

const WINDOW_SIZE: usize = 5;

pub(crate) struct CombinedFilter {
    window: [f64; WINDOW_SIZE],
    index: usize,
    is_filled: bool,
    alpha: f64,
    last_output: f64,
}

impl CombinedFilter {
    pub fn new(alpha: f64) -> Self {
        CombinedFilter {
            window: [0.0; WINDOW_SIZE],
            index: 0,
            is_filled: false,
            alpha,
            last_output: 0.0,
        }
    }

    pub fn update(&mut self, value: f64) -> f64 {
        // 更新窗口
        self.window[self.index] = value;
        self.index = (self.index + 1) % WINDOW_SIZE;
        if self.index == 0 {
            self.is_filled = true;
        }

        // 计算中位数
        let median = if self.is_filled {
            let mut sorted = self.window;
            // 使用插入排序
            for i in 1..WINDOW_SIZE {
                let mut j = i;
                while j > 0 && sorted[j-1] > sorted[j] {
                    sorted.swap(j-1, j);
                    j -= 1;
                }
            }
            sorted[WINDOW_SIZE / 2]
        } else {
            value
        };

        // 低通滤波
        self.last_output = self.alpha * median + (1.0 - self.alpha) * self.last_output;
        
        self.last_output
    }
}
