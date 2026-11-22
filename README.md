# terminal_contract

[![check](https://github.com/nifeos/terminal_contract/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/nifeos/terminal_contract/actions/workflows/ci.yml?query=branch%3Amain)
[![proofs](https://github.com/nifeos/terminal_contract/actions/workflows/proofs.yml/badge.svg?branch=main)](https://github.com/nifeos/terminal_contract/actions/workflows/proofs.yml?query=branch%3Amain)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/nifeos/terminal_contract#license)
[![minimum Rust version](https://img.shields.io/badge/rust-1.88%2B-blue.svg)](https://github.com/nifeos/terminal_contract/blob/main/Cargo.toml)

The request and reply words of a terminal that serves its clients over a message-passing call.
`no_std`, no dependencies, no `unsafe`. The terminal, its clients, its input driver and its
supervisor share this one definition, so none of them can drift from the others.

The crate is the wire half only. It carries the operations, the reply flags, the badges that limit
what a copy of the terminal's endpoint may ask, and the messages a supervisor uses to swap one
terminal for another. The line editor that serves the words lives elsewhere, so a kernel or a
client links the words without linking the editor.

## The protocol

- Every request is a call of two words that blocks until the terminal replies. The first word
  carries the operation in bits 63 to 56 and a length or count in bits 31 to 0. No operation
  gives bits 55 to 32 a meaning yet. `req` builds the word.
- A server reads the first word with `decode`, which answers `None` when the word sets one of
  those bits. The terminal answers that request with `BAD_REQUEST`, as it answers an operation it
  does not implement, so a confused client fails fast instead of hanging.
- Bulk data never rides in the words. A client shares an output page and an input page with the
  terminal, and `OPERATION_WRITE` and `OPERATION_READLINE` name lengths in them. `OPERATION_BYTES`,
  `OPERATION_PRINT`, `OPERATION_READRAW` and `OPERATION_READ_LINE_BYTES` carry up to eight bytes in
  the second word instead, packed little-endian.
- A badge on a copy of the endpoint is an allowlist, and `admits(badge, operation)` is the whole
  table. An unbadged copy serves the client operations. `RAW_ONLY_BADGE` serves raw mode alone,
  and `STDIN_BADGE` serves the line the owner types and nothing else. `SUPERVISOR_BADGE` serves
  `OPERATION_QUIESCE` alone, and no other copy serves it.
- A quiesce stops the terminal so a replacement can take over. A parked read gets `FLAG_RETRY`,
  which `is_retry` reads in both reply shapes. The terminal then waits on its control endpoint for
  `CTL_RESUME`, `CTL_QUIT` or `CTL_HANDOFF`.
- The identity is `INTERFACE_ID` 25 and `VERSION` 1, with no magic. A break bumps the version and
  never takes a new number. A new operation or flag is an addition and keeps the version.

## EXAMPLES

```rust
use terminal_contract::{
    admits, decode, req, BAD_REQUEST, OPERATION_QUIESCE, OPERATION_WRITE, SUPERVISOR_BADGE,
};

// A client asks the terminal to print five bytes from its output page.
let word = req(OPERATION_WRITE, 5);

// The server takes the word apart, refusing any reserved bit, and checks the badge.
assert_eq!(decode(word), Some((OPERATION_WRITE, 5)));
assert_eq!(decode(word | 1 << 40), None); // answered with BAD_REQUEST
assert!(admits(0, OPERATION_WRITE));

// Only the supervisor's copy may quiesce the terminal.
assert!(!admits(0, OPERATION_QUIESCE));
assert!(admits(SUPERVISOR_BADGE, OPERATION_QUIESCE));
assert_eq!(BAD_REQUEST, u64::MAX);
```

## What is proven

Three Kani harnesses in `src/verification.rs` hold for every input, not a sample:

- `decode` answers every first word without panicking, and a word it accepts is the word `req`
  builds from the operation and length it answers. `admits` answers every badge and operation.
- `decode` answers `None` exactly when one of bits 55 to 32 is set.
- `admits` is the table its documentation states, for every badge and every operation.

Each harness has a patch in `falsifications/` that breaks the code it checks and turns that harness
red. The unit tests also pin the wire values of the flags and the identity, and the doctests run
the examples above.

## Building and testing

```sh
cargo test
rustup target add x86_64-unknown-none aarch64-unknown-none-softfloat riscv64imac-unknown-none-elf
cargo build --lib --target x86_64-unknown-none   # also aarch64-unknown-none-softfloat, riscv64imac-unknown-none-elf
```

Run `rustup target add` once, before the first bare-metal build. The tests need no terminal: they
check the words alone. A test that drives a real terminal over these words lives with the program
that serves them.

### Running the proofs

Install Kani 0.68.0, the version CI uses, with `cargo install --locked kani-verifier --version
0.68.0`, then `cargo kani setup`, which downloads a model checker and so needs network access.
After that, `cargo kani` runs the three harnesses in a few seconds once the crate compiles. A
passing run also prints checks marked `UNREACHABLE` and an `internal error: entered unreachable
code` description, which are checks proved unreachable. Only the final line, `Complete - 3
successfully verified harnesses, 0 failures, 3 total.`, matters. To run one harness, use `cargo
kani --harness verification::admits_follows_the_documented_badge_table --exact`.

To replay one falsification patch:

```sh
git apply falsifications/verification.admits_follows_the_documented_badge_table.patch
cargo kani --harness verification::admits_follows_the_documented_badge_table --exact   # ends VERIFICATION:- FAILED
git checkout -- src
```

## BUGS

- The operation names keep the protocol's run-together spellings, such as `OPERATION_READLINE` and
  `OPERATION_INTRCOUNT`, and the control messages keep the `CTL_` prefix. Their names may change
  before the first release; their values will not.
- `OPERATION_INTRCOUNT` and `OPERATION_SUSPEND_COUNT` are polls. A client that wants to learn of a
  `^C` while it waits on something else has to poll them.

## Minimum Rust version

Rust 1.88. To check it:

```sh
rustup toolchain install 1.88 --profile minimal --target x86_64-unknown-none
cargo +1.88 test
cargo +1.88 build --lib --target x86_64-unknown-none
```

## License

Licensed under either of MIT (LICENSE-MIT) or Apache-2.0 (LICENSE-APACHE), at your option.
