//! **Machine-checked proofs (Kani) of the request word and the badge table**: [`decode`] and
//! [`admits`] answer every input without panicking, [`decode`] refuses exactly the words with a
//! reserved bit set, and [`admits`] is the table its documentation states. Behind `#[cfg(kani)]`,
//! so only `cargo kani` compiles them.

use super::*;

/// Falsification: replayable `falsifications/verification.decode_answers_every_word_and_reads_it_back.patch`
/// **[`decode`] answers every first word without panicking**, and a word it accepts is the word
/// [`req`] builds from the operation and length it answers. [`admits`] answers every badge and
/// operation without panicking too.
#[kani::proof]
fn decode_answers_every_word_and_reads_it_back() {
    let word: u64 = kani::any();
    if let Some((operation, length)) = decode(word) {
        assert_eq!(req(operation, length as u64), word);
    }
    let badge: u64 = kani::any();
    let _ = admits(badge, operation(word));
}

/// Falsification: replayable `falsifications/verification.decode_refuses_exactly_the_words_with_a_reserved_bit.patch`
/// **[`decode`] answers `None` exactly when one of bits 55 to 32 is set**, written here from the
/// bit numbers rather than from the crate's own mask.
#[kani::proof]
fn decode_refuses_exactly_the_words_with_a_reserved_bit() {
    let word: u64 = kani::any();
    let reserved = (word >> 32) & 0x00ff_ffff;
    assert_eq!(decode(word).is_none(), reserved != 0);
}

/// Falsification: replayable `falsifications/verification.admits_follows_the_documented_badge_table.patch`
/// **[`admits`] is the table its documentation states**, for every badge and every operation: the
/// unbadged copy serves the eight client operations, [`RAW_ONLY_BADGE`] raw mode alone,
/// [`STDIN_BADGE`] the typed line alone, [`SUPERVISOR_BADGE`] the quiesce alone, and any other
/// badge nothing.
#[kani::proof]
fn admits_follows_the_documented_badge_table() {
    let badge: u64 = kani::any();
    let operation: u64 = kani::any();
    let client = [
        OPERATION_WRITE,
        OPERATION_READLINE,
        OPERATION_BYTES,
        OPERATION_INTRCOUNT,
        OPERATION_SUSPEND_COUNT,
        OPERATION_PRINT,
        OPERATION_RAWMODE,
        OPERATION_READRAW,
    ];
    let expected = if badge == 0 {
        client.contains(&operation)
    } else if badge == RAW_ONLY_BADGE {
        operation == OPERATION_RAWMODE || operation == OPERATION_READRAW
    } else if badge == STDIN_BADGE {
        operation == OPERATION_READ_LINE_BYTES
    } else if badge == SUPERVISOR_BADGE {
        operation == OPERATION_QUIESCE
    } else {
        false
    };
    assert_eq!(admits(badge, operation), expected);
}
