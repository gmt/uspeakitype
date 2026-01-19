//! Shared spectrum computation and color scheme engine
//!
//! This module provides unified FFT-based spectrum analysis and color mapping
//! that can be used by both terminal (ASCII) and graphical (WGPU) renderers.
//! The same computation code drives both representations.

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use std::collections::VecDeque;
use std::sync::Arc;

/// RGBA color with components in [0.0, 1.0]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Convert to ANSI 24-bit color escape sequence (foreground)
    pub fn to_ansi_fg(&self) -> String {
        let r = (self.r * 255.0) as u8;
        let g = (self.g * 255.0) as u8;
        let b = (self.b * 255.0) as u8;
        format!("\x1b[38;2;{};{};{}m", r, g, b)
    }

    /// Convert to ANSI 24-bit color escape sequence (background)
    pub fn to_ansi_bg(&self) -> String {
        let r = (self.r * 255.0) as u8;
        let g = (self.g * 255.0) as u8;
        let b = (self.b * 255.0) as u8;
        format!("\x1b[48;2;{};{};{}m", r, g, b)
    }

    /// Convert to [r, g, b, a] array for WGPU
    pub fn to_array(&self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Linearly interpolate between two colors
    pub fn lerp(a: Color, b: Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        Color {
            r: a.r + (b.r - a.r) * t,
            g: a.g + (b.g - a.g) * t,
            b: a.b + (b.b - a.b) * t,
            a: a.a + (b.a - a.a) * t,
        }
    }
}

/// Trait for color schemes that map intensity [0.0, 1.0] to colors
pub trait ColorScheme: Send + Sync {
    /// Map an intensity value [0.0, 1.0] to a color
    fn color_for_intensity(&self, intensity: f32) -> Color;

    /// Name of this color scheme
    fn name(&self) -> &'static str;
}

/// Flame color scheme following blackbody radiation temperature scale
pub struct FlameScheme;

impl FlameScheme {
    const STOPS: [(f32, Color); 8] = [
        (0.00, Color::rgb(0.0, 0.0, 0.0)),
        (0.14, Color::rgb(0.1, 0.0, 0.3)),
        (0.28, Color::rgb(0.0, 0.2, 0.6)),
        (0.42, Color::rgb(0.0, 0.5, 0.5)),
        (0.56, Color::rgb(0.2, 0.7, 0.2)),
        (0.70, Color::rgb(0.8, 0.8, 0.0)),
        (0.85, Color::rgb(1.0, 0.4, 0.0)),
        (1.00, Color::rgb(1.0, 0.2, 0.2)),
    ];
}

impl ColorScheme for FlameScheme {
    fn color_for_intensity(&self, intensity: f32) -> Color {
        let intensity = intensity.clamp(0.0, 1.0);
        let mut prev = Self::STOPS[0];
        for &(stop_t, stop_color) in &Self::STOPS[1..] {
            if intensity <= stop_t {
                let local_t = (intensity - prev.0) / (stop_t - prev.0);
                return Color::lerp(prev.1, stop_color, local_t);
            }
            prev = (stop_t, stop_color);
        }
        Self::STOPS[Self::STOPS.len() - 1].1
    }

    fn name(&self) -> &'static str {
        "flame"
    }
}

/// Monochrome scheme - white with varying alpha
pub struct MonochromeScheme;

impl ColorScheme for MonochromeScheme {
    fn color_for_intensity(&self, intensity: f32) -> Color {
        let intensity = intensity.clamp(0.0, 1.0);
        Color::new(1.0, 1.0, 1.0, intensity.max(0.1))
    }

    fn name(&self) -> &'static str {
        "mono"
    }
}

/// Ice color scheme - cold blues to white
pub struct IceScheme;

impl IceScheme {
    const STOPS: [(f32, Color); 5] = [
        (0.00, Color::rgb(0.0, 0.0, 0.1)),
        (0.25, Color::rgb(0.0, 0.2, 0.4)),
        (0.50, Color::rgb(0.2, 0.5, 0.7)),
        (0.75, Color::rgb(0.5, 0.8, 0.9)),
        (1.00, Color::rgb(0.9, 0.95, 1.0)),
    ];
}

impl ColorScheme for IceScheme {
    fn color_for_intensity(&self, intensity: f32) -> Color {
        let intensity = intensity.clamp(0.0, 1.0);

        let mut prev = Self::STOPS[0];
        for &(stop_t, stop_color) in &Self::STOPS[1..] {
            if intensity <= stop_t {
                let local_t = (intensity - prev.0) / (stop_t - prev.0);
                return Color::lerp(prev.1, stop_color, local_t);
            }
            prev = (stop_t, stop_color);
        }

        Self::STOPS[Self::STOPS.len() - 1].1
    }

    fn name(&self) -> &'static str {
        "ice"
    }
}

/// Get a color scheme by name
pub fn get_color_scheme(name: &str) -> Box<dyn ColorScheme> {
    match name.to_lowercase().as_str() {
        "flame" | "fire" | "heat" => Box::new(FlameScheme),
        "ice" | "cold" | "blue" => Box::new(IceScheme),
        "mono" | "white" | "grayscale" => Box::new(MonochromeScheme),
        _ => Box::new(FlameScheme),
    }
}

/// Configuration for spectrum analyzer
#[derive(Debug, Clone)]
pub struct SpectrumConfig {
    /// FFT size (must be power of 2)
    pub fft_size: usize,
    /// Hop size between FFT frames (typically fft_size / 2)
    pub hop_size: usize,
    /// Sample rate in Hz
    pub sample_rate: f32,
    /// Number of frequency bands to compute
    pub num_bands: usize,
    /// Minimum frequency in Hz
    pub min_freq: f32,
    /// Maximum frequency in Hz
    pub max_freq: f32,
    /// Use logarithmic frequency scaling (better for audio)
    pub log_frequency: bool,
    /// Smoothing factor for band levels (0.0 = no smoothing, 1.0 = infinite smoothing)
    pub smoothing: f32,
}

impl Default for SpectrumConfig {
    fn default() -> Self {
        Self {
            fft_size: 1024,
            hop_size: 512,
            sample_rate: 16000.0,
            num_bands: 32,
            min_freq: 60.0,
            max_freq: 7500.0,
            log_frequency: true,
            smoothing: 0.3,
        }
    }
}

/// Computed spectrum data - the output of the analyzer
/// This struct is used by both ASCII and WGPU renderers
#[derive(Debug, Clone)]
pub struct SpectrumData {
    /// Frequency band magnitudes, normalized to [0.0, 1.0]
    /// Index 0 = lowest frequency, Index N-1 = highest frequency
    pub bands: Vec<f32>,
    /// Peak level across all bands (for VU meter style display)
    pub peak: f32,
    /// Whether voice activity is detected
    pub is_active: bool,
    /// Timestamp when this data was computed
    pub timestamp: std::time::Instant,
}

impl SpectrumData {
    pub fn new(num_bands: usize) -> Self {
        Self {
            bands: vec![0.0; num_bands],
            peak: 0.0,
            is_active: false,
            timestamp: std::time::Instant::now(),
        }
    }

    /// Get color for a band at a given level using the provided color scheme
    #[inline]
    pub fn color_for_band(&self, band_idx: usize, scheme: &dyn ColorScheme) -> Color {
        let intensity = self.bands.get(band_idx).copied().unwrap_or(0.0);
        scheme.color_for_intensity(intensity)
    }
}

/// Spectrum analyzer that computes frequency bands from audio samples
pub struct SpectrumAnalyzer {
    config: SpectrumConfig,
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    ring_buffer: VecDeque<f32>,
    band_edges: Vec<usize>,
    current_data: SpectrumData,
    smoothed_bands: Vec<f32>,
}

impl SpectrumAnalyzer {
    pub fn new(config: SpectrumConfig) -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(config.fft_size);

        let window: Vec<f32> = (0..config.fft_size)
            .map(|i| {
                let phase = 2.0 * std::f32::consts::PI * i as f32 / (config.fft_size - 1) as f32;
                0.5 - 0.5 * phase.cos() // Hann window
            })
            .collect();

        let band_edges = Self::compute_band_edges(&config);

        let current_data = SpectrumData::new(config.num_bands);
        let smoothed_bands = vec![0.0; config.num_bands];

        let ring_buffer_capacity = config.fft_size * 2;
        Self {
            config,
            fft,
            window,
            ring_buffer: VecDeque::with_capacity(ring_buffer_capacity),
            band_edges,
            current_data,
            smoothed_bands,
        }
    }

    fn compute_band_edges(config: &SpectrumConfig) -> Vec<usize> {
        let nyquist = config.sample_rate / 2.0;
        let bin_hz = nyquist / (config.fft_size / 2) as f32;
        let mut edges = Vec::with_capacity(config.num_bands + 1);

        if config.log_frequency {
            let log_min = config.min_freq.ln();
            let log_max = config.max_freq.ln();
            let log_step = (log_max - log_min) / config.num_bands as f32;

            for i in 0..=config.num_bands {
                let freq = (log_min + i as f32 * log_step).exp();
                let bin = ((freq / bin_hz).round() as usize).min(config.fft_size / 2 - 1);
                edges.push(bin);
            }
        } else {
            let freq_step = (config.max_freq - config.min_freq) / config.num_bands as f32;

            for i in 0..=config.num_bands {
                let freq = config.min_freq + i as f32 * freq_step;
                let bin = ((freq / bin_hz).round() as usize).min(config.fft_size / 2 - 1);
                edges.push(bin);
            }
        }

        edges
    }

    pub fn push_samples(&mut self, samples: &[f32]) {
        for &sample in samples {
            if self.ring_buffer.len() >= self.config.fft_size * 2 {
                self.ring_buffer.pop_front();
            }
            self.ring_buffer.push_back(sample);
        }
    }

    pub fn process(&mut self) -> bool {
        if self.ring_buffer.len() < self.config.fft_size {
            return false;
        }

        let mut computed = false;

        while self.ring_buffer.len() >= self.config.fft_size {
            self.process_frame();
            computed = true;
            for _ in 0..self.config.hop_size {
                self.ring_buffer.pop_front();
            }
        }

        computed
    }

    fn process_frame(&mut self) {
        let fft_size = self.config.fft_size;
        let mut frame: Vec<Complex<f32>> = Vec::with_capacity(fft_size);
        let base = self.ring_buffer.len().saturating_sub(fft_size);

        for i in 0..fft_size {
            let sample = self.ring_buffer.get(base + i).copied().unwrap_or(0.0);
            frame.push(Complex::new(sample * self.window[i], 0.0));
        }

        self.fft.process(&mut frame);

        let magnitudes: Vec<f32> = frame.iter().take(fft_size / 2).map(|c| c.norm()).collect();

        self.compute_bands(&magnitudes);
    }

    fn compute_bands(&mut self, magnitudes: &[f32]) {
        let num_bands = self.config.num_bands;
        let smoothing = self.config.smoothing;
        let mut peak = 0.0f32;

        for band in 0..num_bands {
            let start_bin = self.band_edges[band];
            let end_bin = self.band_edges[band + 1].max(start_bin + 1);

            let mut band_mag = 0.0f32;
            for &mag in magnitudes
                .iter()
                .take(end_bin.min(magnitudes.len()))
                .skip(start_bin)
            {
                band_mag = band_mag.max(mag);
            }

            // dB conversion: 20 * log10(mag), map [-60dB, 0dB] to [0, 1]
            let db = if band_mag > 1e-10 {
                20.0 * band_mag.log10()
            } else {
                -100.0
            };
            let normalized = ((db + 60.0) / 60.0).clamp(0.0, 1.0);

            self.smoothed_bands[band] =
                self.smoothed_bands[band] * smoothing + normalized * (1.0 - smoothing);

            self.current_data.bands[band] = self.smoothed_bands[band];
            peak = peak.max(self.current_data.bands[band]);
        }

        self.current_data.peak = peak;
        self.current_data.is_active = peak > 0.15;
        self.current_data.timestamp = std::time::Instant::now();
    }

    pub fn data(&self) -> &SpectrumData {
        &self.current_data
    }

    pub fn data_mut(&mut self) -> &mut SpectrumData {
        &mut self.current_data
    }

    pub fn config(&self) -> &SpectrumConfig {
        &self.config
    }

    pub fn reset(&mut self) {
        self.ring_buffer.clear();
        self.smoothed_bands.fill(0.0);
        self.current_data.bands.fill(0.0);
        self.current_data.peak = 0.0;
        self.current_data.is_active = false;
    }
}

pub struct WaterfallHistory {
    columns: VecDeque<Vec<f32>>,
    max_columns: usize,
    num_bands: usize,
}

impl WaterfallHistory {
    pub fn new(max_columns: usize, num_bands: usize) -> Self {
        Self {
            columns: VecDeque::with_capacity(max_columns),
            max_columns,
            num_bands,
        }
    }

    pub fn push(&mut self, bands: &[f32]) {
        if self.columns.len() >= self.max_columns {
            self.columns.pop_front();
        }
        self.columns.push_back(bands.to_vec());
    }

    pub fn get(&self, col: usize) -> Option<&[f32]> {
        self.columns.get(col).map(|v| v.as_slice())
    }

    #[inline]
    pub fn get_intensity(&self, col: usize, band: usize) -> f32 {
        self.columns
            .get(col)
            .and_then(|c| c.get(band))
            .copied()
            .unwrap_or(0.0)
    }

    #[inline]
    pub fn get_color(&self, col: usize, band: usize, scheme: &dyn ColorScheme) -> Color {
        let intensity = self.get_intensity(col, band);
        scheme.color_for_intensity(intensity)
    }

    pub fn len(&self) -> usize {
        self.columns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.max_columns
    }

    pub fn num_bands(&self) -> usize {
        self.num_bands
    }

    pub fn clear(&mut self) {
        self.columns.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &Vec<f32>> {
        self.columns.iter()
    }
}

#[inline]
pub fn quantize_intensity(intensity: f32, num_levels: usize) -> usize {
    let intensity = intensity.clamp(0.0, 1.0);
    let level = (intensity * (num_levels as f32 - 0.001)).floor() as usize;
    level.min(num_levels - 1)
}

#[inline]
pub fn intensity_to_height(intensity: f32, max_height: f32) -> f32 {
    intensity.clamp(0.0, 1.0) * max_height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flame_scheme_bounds() {
        let scheme = FlameScheme;
        let c0 = scheme.color_for_intensity(0.0);
        let c1 = scheme.color_for_intensity(1.0);
        assert!(c0.r < 0.1 && c0.g < 0.1 && c0.b < 0.1);
        assert!(c1.r > 0.8);
    }

    #[test]
    fn color_lerp_works() {
        let black = Color::rgb(0.0, 0.0, 0.0);
        let white = Color::rgb(1.0, 1.0, 1.0);
        let mid = Color::lerp(black, white, 0.5);
        assert!((mid.r - 0.5).abs() < 0.01);
        assert!((mid.g - 0.5).abs() < 0.01);
        assert!((mid.b - 0.5).abs() < 0.01);
    }

    #[test]
    fn quantize_intensity_bounds() {
        assert_eq!(quantize_intensity(0.0, 8), 0);
        assert_eq!(quantize_intensity(1.0, 8), 7);
        assert_eq!(quantize_intensity(-0.5, 8), 0);
        assert_eq!(quantize_intensity(1.5, 8), 7);
        let mid = quantize_intensity(0.5, 8);
        assert!(mid >= 3 && mid <= 4);
    }

    #[test]
    fn spectrum_analyzer_processes() {
        let config = SpectrumConfig {
            fft_size: 256,
            hop_size: 128,
            num_bands: 8,
            ..Default::default()
        };
        let mut analyzer = SpectrumAnalyzer::new(config);
        let samples: Vec<f32> = (0..512).map(|i| (i as f32 * 0.1).sin()).collect();
        analyzer.push_samples(&samples);
        assert!(analyzer.process());
        assert_eq!(analyzer.data().bands.len(), 8);
    }

    #[test]
    fn waterfall_history_capacity() {
        let mut history = WaterfallHistory::new(3, 4);
        history.push(&[0.1, 0.2, 0.3, 0.4]);
        history.push(&[0.2, 0.3, 0.4, 0.5]);
        history.push(&[0.3, 0.4, 0.5, 0.6]);
        assert_eq!(history.len(), 3);
        history.push(&[0.4, 0.5, 0.6, 0.7]);
        assert_eq!(history.len(), 3);
        assert!((history.get_intensity(0, 0) - 0.2).abs() < 0.01);
    }

    #[test]
    fn ansi_color_format() {
        let red = Color::rgb(1.0, 0.0, 0.0);
        assert_eq!(red.to_ansi_fg(), "\x1b[38;2;255;0;0m");
    }
}
