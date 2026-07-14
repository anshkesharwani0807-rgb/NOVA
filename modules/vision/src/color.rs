use async_trait::async_trait;
use nova_kernel::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DominantColor {
    pub color: RgbColor,
    pub percentage: f64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorResult {
    pub dominant_colors: Vec<DominantColor>,
    pub palette: Vec<RgbColor>,
    pub average_color: RgbColor,
    pub colorfulness: f64,
}

#[async_trait]
pub trait ColorAnalyzer: Send + Sync {
    async fn analyze(&self, bytes: &[u8]) -> Result<ColorResult>;
}

pub struct MockColorAnalyzer;

impl MockColorAnalyzer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockColorAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ColorAnalyzer for MockColorAnalyzer {
    async fn analyze(&self, _bytes: &[u8]) -> Result<ColorResult> {
        Ok(ColorResult {
            dominant_colors: vec![
                DominantColor {
                    color: RgbColor {
                        r: 100,
                        g: 150,
                        b: 200,
                    },
                    percentage: 0.35,
                    name: "steel blue".to_string(),
                },
                DominantColor {
                    color: RgbColor {
                        r: 200,
                        g: 180,
                        b: 100,
                    },
                    percentage: 0.25,
                    name: "tan".to_string(),
                },
                DominantColor {
                    color: RgbColor {
                        r: 50,
                        g: 80,
                        b: 120,
                    },
                    percentage: 0.20,
                    name: "dark blue".to_string(),
                },
            ],
            palette: vec![
                RgbColor {
                    r: 100,
                    g: 150,
                    b: 200,
                },
                RgbColor {
                    r: 200,
                    g: 180,
                    b: 100,
                },
                RgbColor {
                    r: 50,
                    g: 80,
                    b: 120,
                },
                RgbColor {
                    r: 220,
                    g: 220,
                    b: 220,
                },
                RgbColor {
                    r: 30,
                    g: 30,
                    b: 30,
                },
            ],
            average_color: RgbColor {
                r: 120,
                g: 130,
                b: 140,
            },
            colorfulness: 0.55,
        })
    }
}
