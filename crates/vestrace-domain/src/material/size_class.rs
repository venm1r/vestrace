/// A padded plaintext-size bucket. The variant contains no exact byte count.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeClass {
    FourKiB,
    EightKiB,
    SixteenKiB,
    ThirtyTwoKiB,
    SixtyFourKiB,
    OneTwentyEightKiB,
    TwoFiftySixKiB,
    FiveTwelveKiB,
    OneMiB,
    TwoMiB,
    FourMiB,
    EightMiB,
    SixteenMiB,
    ThirtyTwoMiB,
    SixtyFourMiB,
    OneTwentyEightMiB,
    TwoFiftySixMiB,
    FiveTwelveMiB,
    OneGiBOrLarger,
}

impl SizeClass {
    /// The smallest padded plaintext length represented by this class.
    pub const fn minimum_bytes(self) -> usize {
        match self {
            Self::FourKiB => 4096,
            Self::EightKiB => 8192,
            Self::SixteenKiB => 16_384,
            Self::ThirtyTwoKiB => 32_768,
            Self::SixtyFourKiB => 65_536,
            Self::OneTwentyEightKiB => 131_072,
            Self::TwoFiftySixKiB => 262_144,
            Self::FiveTwelveKiB => 524_288,
            Self::OneMiB => 1_048_576,
            Self::TwoMiB => 2_097_152,
            Self::FourMiB => 4_194_304,
            Self::EightMiB => 8_388_608,
            Self::SixteenMiB => 16_777_216,
            Self::ThirtyTwoMiB => 33_554_432,
            Self::SixtyFourMiB => 67_108_864,
            Self::OneTwentyEightMiB => 134_217_728,
            Self::TwoFiftySixMiB => 268_435_456,
            Self::FiveTwelveMiB => 536_870_912,
            Self::OneGiBOrLarger => 1_073_741_824,
        }
    }
}

/// Returns a padded power-of-two size class with a mandatory 4 KiB minimum.
pub const fn size_class_for(byte_len: usize) -> SizeClass {
    match byte_len {
        0..=4096 => SizeClass::FourKiB,
        4097..=8192 => SizeClass::EightKiB,
        8193..=16_384 => SizeClass::SixteenKiB,
        16_385..=32_768 => SizeClass::ThirtyTwoKiB,
        32_769..=65_536 => SizeClass::SixtyFourKiB,
        65_537..=131_072 => SizeClass::OneTwentyEightKiB,
        131_073..=262_144 => SizeClass::TwoFiftySixKiB,
        262_145..=524_288 => SizeClass::FiveTwelveKiB,
        524_289..=1_048_576 => SizeClass::OneMiB,
        1_048_577..=2_097_152 => SizeClass::TwoMiB,
        2_097_153..=4_194_304 => SizeClass::FourMiB,
        4_194_305..=8_388_608 => SizeClass::EightMiB,
        8_388_609..=16_777_216 => SizeClass::SixteenMiB,
        16_777_217..=33_554_432 => SizeClass::ThirtyTwoMiB,
        33_554_433..=67_108_864 => SizeClass::SixtyFourMiB,
        67_108_865..=134_217_728 => SizeClass::OneTwentyEightMiB,
        134_217_729..=268_435_456 => SizeClass::TwoFiftySixMiB,
        268_435_457..=536_870_912 => SizeClass::FiveTwelveMiB,
        _ => SizeClass::OneGiBOrLarger,
    }
}
