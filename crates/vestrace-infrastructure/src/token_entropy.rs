//! The randomness a new credential is built from.
//!
//! Separated into its own adapter behind a port so that the source of a
//! credential's entropy is one named, wired thing rather than a call buried in
//! a handler. A weak source here produces credentials that look fine and can be
//! guessed, and that failure is invisible at every other layer.

use ring::rand::{SecureRandom, SystemRandom};
use vestrace_application::{ApplicationError, TokenEntropySource};

/// The operating system's CSPRNG, via `ring`.
pub struct SystemTokenEntropy {
    random: SystemRandom,
}

impl SystemTokenEntropy {
    pub fn new() -> Self {
        Self {
            random: SystemRandom::new(),
        }
    }
}

impl Default for SystemTokenEntropy {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SystemTokenEntropy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("SystemTokenEntropy").finish()
    }
}

impl TokenEntropySource for SystemTokenEntropy {
    fn fill(&self, buffer: &mut [u8]) -> Result<(), ApplicationError> {
        // Propagated rather than swallowed. A caller that receives an error
        // mints nothing; a caller handed a zeroed buffer would mint a
        // credential an attacker can reproduce exactly.
        self.random.fill(buffer).map_err(|_| {
            ApplicationError::Unavailable(
                "the system random number generator is unavailable".into(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buffer_is_filled_and_not_left_zeroed() {
        let source = SystemTokenEntropy::new();
        let mut buffer = [0u8; 32];
        source.fill(&mut buffer).unwrap();
        assert!(
            buffer.iter().any(|byte| *byte != 0),
            "a zeroed buffer would mint a credential anyone can reproduce"
        );
    }

    #[test]
    fn two_draws_differ() {
        // Not a randomness test — it catches a source wired to a constant,
        // which is the failure that actually happens.
        let source = SystemTokenEntropy::new();
        let mut first = [0u8; 32];
        let mut second = [0u8; 32];
        source.fill(&mut first).unwrap();
        source.fill(&mut second).unwrap();
        assert_ne!(first, second);
    }
}
