#![allow(unstable)]

extern crate libc;
use self::libc::{c_int};
use std::num::Float;
use std::f64::consts::PI;
use std::f64;

/*
This module is responsible for a number of tasks:
1. Split the audio channels out of the stereo data
2. Translate the audio to f64 data
3. Appy a window function?! :/
4. Compute the FFT
5. Compute the equalizer bands from the FFT output
*/


// make sure we're as good as this asshole:
// http://www.swharden.com/blog/2013-05-09-realtime-fft-audio-visualization-with-python/



mod ext {
    extern crate libc;
    use self::libc::{c_int};
    use super::{FftwPlan, FftwComplex};


    #[link(name="fftw3")]
    extern {
        pub fn fftw_plan_dft_r2c_1d(n: c_int, input: *mut f64, output: *mut FftwComplex, flags: c_int) -> *const FftwPlan;
        pub fn fftw_execute(plan: *const FftwPlan);
    }
}


/// {FFTW_ESTIMATE} or 64. Specifies that, instead of actual measurements of
/// different algorithms, a simple heuristic is used to pick a (probably
/// sub-optimal) plan quickly. With this flag, the input/output arrays are not
/// overwritten during planning. It is the default value
const FFTW_ESTIMATE: c_int = (1 << 6);
/// FFTW_MEASURE or 0. tells FFTW to find an optimized plan by actually
/// computing several FFTs and measuring their execution time. Depending on
/// your machine, this can take some time (often a few seconds).
const FFTW_MEASURE: c_int = 0;
/// FFTW_PATIENT or 32. It is like "FFTW_MEASURE", but considers a wider range
/// of algorithms and often produces a “more optimal” plan (especially for large
/// transforms), but at the expense of several times longer planning time
/// (especially for large transforms).
const FFTW_PATIENT: c_int = 32;
/// FFTW_EXHAUSTIVE or 8. It is like "FFTW_PATIENT", but considers an even wider
/// range of algorithms, including many that we think are unlikely to be fast,
/// to produce the most optimal plan but with a substantially increased planning
/// time.
const FFTW_EXHAUSTIVE: c_int = 8;



#[derive(Copy)]
pub enum FftwPlan {}


#[repr(C)]
#[derive(Copy)]
struct FftwComplex {
    re: f64,
    im: f64
}


impl FftwComplex {
    pub fn abs(&self) -> f64 {
        ((self.re * self.re) + (self.im * self.im)).sqrt()
    }
}


fn is_power_of_two(x: usize) -> bool {
    (x != 0) && ((x & (x - 1)) == 0)
}


#[test]
fn test_pwer_two() {
    assert!(is_power_of_two(1024));
    assert!(is_power_of_two(512));
    assert!(is_power_of_two(2));
    assert!(is_power_of_two(4));
    assert!(is_power_of_two(8));
    assert!(is_power_of_two(16));
    assert!(is_power_of_two(32));
    assert!(!is_power_of_two(1));
    assert!(!is_power_of_two(7));
    assert!(!is_power_of_two(500));
}


/// Scales down a vector by averaging the elements between the resulting points
pub fn scale_fft_output(input: &Vec<f64>, new_len: usize) -> Vec<f64> {
    if new_len >= input.len() {
        return input.clone();
    }

    let band_size: usize = input.len() / new_len;
    assert!(band_size > 0);
    let mut output: Vec<f64> = Vec::with_capacity(new_len);

    let mut temp_count: usize = 0;
    let mut sum: f64 = 0.0;

    for &x in input.iter() {
        if temp_count >= band_size {
            let avg: f64 = sum/temp_count as f64;
            output.push(avg);
            temp_count = 0;
            sum = 0.0;
        } else {
            sum += x;
            temp_count+=1;
        }
    }

    if temp_count >= band_size {
        output.push(sum/temp_count as f64);
    }

    output
}




pub struct AudioFFT<'a> {
    channels: usize,
    input: Vec<f64>,
    output: Vec<FftwComplex>,
    plan: *const FftwPlan,
    n: usize,
}


impl<'a> AudioFFT<'a> {
    pub fn new(n: usize, channels: usize) -> AudioFFT<'a> {
        if !is_power_of_two(n) {
            panic!("n should be a power of two!");
        }

        // input is the data to feed to the FFT
        let mut input: Vec<f64> = Vec::with_capacity(n);
        // output is where the FFT puts its data.
        // FFTs are symmetrical and the real FFT optimizes by returning a
        // half-length array rather than doing extra computation
        let mut output: Vec<FftwComplex> = Vec::with_capacity(n/2);

        // initialize the arrays.
        for _ in range(0, n) {
            input.push(0f64);
        }
        for _ in range(0, n/2) {
             output.push(FftwComplex{im:0f64,re:0f64});
        }

        let plan = unsafe { ext::fftw_plan_dft_r2c_1d(n as i32, input.as_mut_ptr(), output.as_mut_ptr(), FFTW_MEASURE)};

        AudioFFT {
            channels: channels,
            input: input,
            output: output,
            plan: plan,
            n: n
        }
    }

    /// Returns the amount of data we need to make this work.
    pub fn get_buf_size(&self) -> usize {
        const BYTES_PER_SAMPLE: usize = 2; // 16 bit
        self.n * BYTES_PER_SAMPLE * self.channels
    }

    /// Turns a slice of u8 into a Vec<f64> of half the length
    /// (Reads the i16 values out of the buffer, then casts them to f64)
    fn get_floats(&self, buffer: &[u8]) -> Vec<f64> {
        let short_vec: Vec<i16> = unsafe{ Vec::from_raw_buf(buffer.as_ptr() as *const i16, buffer.len()/2) };
        let mut float_vec: Vec<f64> = Vec::with_capacity(short_vec.len());
        for val in short_vec.iter() {
            float_vec.push(*val as f64);
        }
        float_vec
    }

    /// Splits audio data channels out into separate vectors
    /// For stereo, these means producing a vector of two vectors, where the
    /// first vector is the audio data for the left channel and the second
    /// vector is the audio data for the right channel
    fn split_channels(&self, all_floats: &Vec<f64>) -> Vec<Vec<f64>> {
        let mut out: Vec<Vec<f64>> = Vec::new();
        for _ in range(0, self.channels) {
            out.push(Vec::with_capacity(all_floats.len()/self.channels));
        }
        for (i, &val) in all_floats.iter().enumerate() {
            out[i % self.channels].push(val);
        }
        out
    }

    /// Loads an audo channel's vector into the input for the FFT
    fn load_channel(&mut self, channel_data: &Vec<f64>) {
        for (i, &val) in channel_data.iter().enumerate() {
            self.input[i] = val;
        }
    }

    /// Modifies a vector in-place with the hanning window function
    /// This prevents spectral leakage
    fn do_hanning_window(&self, channel_data: &mut Vec<f64>) {
        let divider: f64 = (channel_data.len() - 1) as f64;

        for (i, val) in channel_data.iter_mut().enumerate() {
            let cos_inner: f64 = 2.0 * PI * (i as f64) / divider;
            let cos_part: f64 = cos_inner.cos();
            let multiplier: f64 = 0.5 * (1.0 - cos_part);
            *val = *val * multiplier;
        }
    }

    /// Reads the output from the FFT and converts it into averages of parts of
    /// the power spectrum. (Ex: an equalizer visualizer).
    /// This function may need some work.
    fn get_output(&self) -> Vec<f64> {
        // Convert the FFT data into decibals (power)
        self.output.iter().map(|x| 20.0 * x.abs().log10()).collect()
    }

    /// Turn a buffer into equalizer data.
    pub fn execute(&mut self, buffer: &[u8]) -> Vec<f64> {
        if buffer.len() != self.get_buf_size() {
            panic!("incorrect buffer length");
        }
        let all_floats = self.get_floats(buffer);
        let mut channel_data = self.split_channels(&all_floats);
        self.do_hanning_window(&mut channel_data[0]);
        self.load_channel(&channel_data[0]);

        unsafe { ext::fftw_execute(self.plan) };
        self.get_output()
    }

}
