//! Boot policy for the Linux desktop. UI geometry is authored in logical units.
use super::{AppError, AppResult};
use crate::core::SizeI;
use crate::platform::ScaleFactor;

/// Output density policy. Fixed values are factors: 1.0 = 100%, 2.0 = 200%.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum OutputScale {
    /// Estimate density from KMS physical dimensions, relative to 96 pixels/inch, rounded to
    /// 25% steps and clamped to 100–400%. Missing or implausible dimensions use 100%.
    #[default]
    Auto,
    /// Explicit 100–400% scale, quantized to Wayland's 1/120 increments.
    Fixed(f32),
}

impl OutputScale {
    pub(crate) fn validate(self) -> AppResult<()> {
        if let Self::Fixed(value) = self
            && (!value.is_finite() || !(1.0..=4.0).contains(&value))
        {
            return Err(AppError::new(
                "output scale must be finite and between 1.0 and 4.0",
            ));
        }
        Ok(())
    }

    pub(crate) fn resolve(self, pixels: SizeI, millimeters: SizeI) -> AppResult<ScaleFactor> {
        self.validate()?;
        let value = match self {
            Self::Fixed(value) => (value * 120.0).round() / 120.0,
            Self::Auto => {
                let x = pixels.width as f32 * 25.4 / millimeters.width as f32;
                let y = pixels.height as f32 * 25.4 / millimeters.height as f32;
                let plausible = (50..=3000).contains(&millimeters.width)
                    && (50..=3000).contains(&millimeters.height)
                    && (50.0..=500.0).contains(&x)
                    && (50.0..=500.0).contains(&y)
                    && (0.9..=1.1).contains(&(x / y));
                if plausible {
                    (((x * y).sqrt() / 96.0) * 4.0).round().clamp(4.0, 16.0) / 4.0
                } else {
                    1.0
                }
            }
        };
        ScaleFactor::new(value).map_err(|error| AppError::new(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boot_scale_uses_density_instead_of_resolution_alone() {
        let full_hd = SizeI {
            width: 1920,
            height: 1080,
        };
        let uhd = SizeI {
            width: 3840,
            height: 2160,
        };
        let desktop = SizeI {
            width: 531,
            height: 299,
        }; // 24 inch
        assert_eq!(
            OutputScale::Auto.resolve(full_hd, desktop).unwrap().get(),
            1.0
        );
        assert_eq!(OutputScale::Auto.resolve(uhd, desktop).unwrap().get(), 2.0);
        assert_eq!(
            OutputScale::Auto
                .resolve(
                    uhd,
                    SizeI {
                        width: 941,
                        height: 529
                    }
                )
                .unwrap()
                .get(),
            1.0
        );
        assert_eq!(
            OutputScale::Auto
                .resolve(
                    uhd,
                    SizeI {
                        width: 597,
                        height: 336
                    }
                )
                .unwrap()
                .get(),
            1.75
        );
    }
    #[test]
    fn fixed_scale_is_quantized_to_the_announced_protocol_value() {
        let scale = OutputScale::Fixed(1.333)
            .resolve(SizeI::default(), SizeI::default())
            .unwrap();
        assert_eq!((scale.get() * 120.0).round() as u32, 160);
        assert_eq!(scale.get(), 160.0 / 120.0);
        assert_eq!(scale.get().ceil() as i32, 2);
    }

    #[test]
    fn invalid_edid_falls_back_and_explicit_preference_wins() {
        let pixels = SizeI {
            width: 3840,
            height: 2160,
        };
        for size in [
            SizeI::default(),
            SizeI {
                width: 1,
                height: 1,
            },
            SizeI {
                width: 500,
                height: 500,
            },
            SizeI {
                width: -1,
                height: 300,
            },
        ] {
            assert_eq!(OutputScale::Auto.resolve(pixels, size).unwrap().get(), 1.0);
            assert_eq!(
                OutputScale::Fixed(1.5).resolve(pixels, size).unwrap().get(),
                1.5
            );
        }
        for scale in [0.0, -1.0, f32::NAN, f32::INFINITY, 4.1] {
            assert!(OutputScale::Fixed(scale).validate().is_err());
        }
    }
}
