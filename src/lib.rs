//! The inter-process communication (IPC) framing of a terminal served over a message-passing
//! call: the operations, flags, badges and limits shared by the terminal, its clients, its input
//! driver and its supervisor. This is a **userspace protocol**, not a kernel's application binary
//! interface (ABI): a kernel routes these words without reading them.
//!
//! A wire-only leaf: it depends on nothing, so a kernel and every client link the words without
//! linking the line editor that serves them.
//!
//! Every request is a call that sends two words and blocks until the terminal replies. The first
//! word packs an operation and a length or count ([`req`]); a server takes it apart with
//! [`decode`]. Bulk data never rides in the words: a client shares an output page (it writes, the
//! terminal reads) and an input page (the terminal writes, it reads) with the terminal.

#![no_std]

/// Request opcode, in bits 63:56 of the first `CALL` word. The low 32 bits carry a length
/// or count; bits 55:32 are reserved and must be zero, and a terminal refuses a request that sets
/// one ([`decode`]).
pub const OPERATION_SHIFT: u32 = 56;
/// Application → terminal: print `len` bytes from the client's output page. Reply when the
/// bytes are on the wire: r0 = bytes consumed.
pub const OPERATION_WRITE: u64 = 1;
/// Application → terminal: read one line. The low bits carry a prompt length (0 for none);
/// the prompt bytes are in the client's output page. The reply comes when a line is ready:
/// r0 = line length (the bytes are in the client's input page), r1 = [`FLAG_EOF`] /
/// [`FLAG_INTERRUPTED`] or 0.
pub const OPERATION_READLINE: u64 = 2;
/// Input driver → terminal: `count` (1..=8) raw wire bytes, packed little-endian in the
/// second word. Replied immediately (r0 = 0); the rendezvous is the flow control.
pub const OPERATION_BYTES: u64 = 3;
/// Application → terminal: how many `^C` has the terminal seen since boot? Replied immediately
/// (r0 = the running count). This is a shell's `^C` sensor while a foreground job runs and no
/// read is parked to fail: the shell polls it and escalates its interrupt as the count advances.
/// It is a poll, not a delivered signal, so a shell with no non-blocking receive can watch two
/// things (the job and `^C`) at once by polling and yielding.
pub const OPERATION_INTRCOUNT: u64 = 4;
/// Application → terminal: how many `^Z` has the terminal seen since boot? Replied immediately
/// (r0 = the running count). [`OPERATION_INTRCOUNT`]'s twin for job control: a shell polls it
/// while it watches a foreground job, and suspends the job when the count advances. Only the line
/// discipline counts, so a `^Z` in raw mode is a byte to the reader.
pub const OPERATION_SUSPEND_COUNT: u64 = 9;
/// Adapter → terminal: print `len` (1..=8) bytes carried **in registers**, packed
/// little-endian in the second word. Replied when the bytes are on the wire: r0 = bytes
/// consumed, exactly as [`OPERATION_WRITE`] answers.
///
/// It gives the terminal a second writer without a second page: [`OPERATION_WRITE`] reads the one
/// client output page, which belongs to the program that owns the terminal. Eight bytes is the
/// request shape, not a choice: a served request arrives with two data words, the first of them
/// the request word, so a sixteen-byte message from another writer is two of these.
pub const OPERATION_PRINT: u64 = 5;

/// Application → terminal: switch the terminal between line-discipline and raw mode. `len` is 1
/// to enter raw mode, 0 to leave it. Replied immediately, r0 = 0.
///
/// While raw mode is on, [`OPERATION_BYTES`] bypasses the line editor entirely, and each burst
/// the driver delivers reaches the application through [`OPERATION_READRAW`]. `OPERATION_READLINE`
/// is refused (`BAD_REQUEST`) in raw mode and `OPERATION_READRAW` outside it. Switching either way
/// abandons the line in progress in the mode being left, as `^C` does; history and the kill buffer
/// are untouched. A full-screen editor is the client this mode is for.
pub const OPERATION_RAWMODE: u64 = 6;

/// Application → terminal: read raw bytes, no line discipline. Valid only while raw mode
/// ([`OPERATION_RAWMODE`]) is on; refused with `BAD_REQUEST` otherwise. Replied when at least one
/// byte is available, never held back to fill more (raw mode delivers a keystroke at a time and
/// never batches): r0 = byte count (1..=8), r1 = the bytes packed little-endian, the same
/// register-only shape [`OPERATION_BYTES`] and [`OPERATION_PRINT`] already use, so this needs no
/// page either. **At most one read may be outstanding**, exactly like [`OPERATION_READLINE`]; a
/// second one while one is parked is refused with `BAD_REQUEST`.
pub const OPERATION_READRAW: u64 = 7;

/// The badge on a copy of the terminal endpoint that reads keystrokes and nothing else.
///
/// The terminal serves a badge by its value: on this one only [`OPERATION_RAWMODE`] and
/// [`OPERATION_READRAW`], refusing the rest with [`BAD_REQUEST`], and on an unknown one nothing.
/// The program that starts sessions mints it for a session that draws its own screen and has no
/// keyboard of its own. A kernel that refuses to re-badge a badged copy keeps its holder from
/// widening it back.
pub const RAW_ONLY_BADGE: u64 = 1;

/// **The badge on a copy of the terminal endpoint that is a program's standard input.** The
/// terminal serves [`OPERATION_READ_LINE_BYTES`] on it and refuses everything else with
/// [`BAD_REQUEST`], so its holder reads the lines the owner types and can neither type one, print,
/// nor switch modes. The program that starts processes mints it only for a program its catalog
/// marks as reading the console.
pub const STDIN_BADGE: u64 = 2;

/// Application → terminal, on a [`STDIN_BADGE`] copy only: **up to eight bytes of the line being
/// typed**. The terminal's own line discipline echoes and edits the line; once it is complete, the
/// reply is r0 = count (1..=8) and r1 = the bytes, packed little-endian as [`OPERATION_READRAW`]
/// packs them. Later calls drain the rest of the line, its `\n` included, before the next line is
/// taken. r0 = 0 is end of input: `^D`, a `^C`, a quiesce, or raw mode, none of which a reader can
/// be made to wait through. Refused in raw mode, and while another such read is parked. No page is
/// involved.
pub const OPERATION_READ_LINE_BYTES: u64 = 10;

/// Supervisor → terminal: **stop serving so a replacement can take over**. The served endpoint's
/// first-in, first-out queue does the draining. Parked [`OPERATION_READLINE`] and
/// [`OPERATION_READRAW`] get [`FLAG_RETRY`] (a reply capability cannot leave the process), the
/// reply is r0 = [`QUIESCED`], and the terminal waits for [`CTL_RESUME`] or [`CTL_QUIT`] on its
/// control endpoint. A terminal with none refuses it with [`BAD_REQUEST`], and so does every
/// terminal on a copy not badged [`SUPERVISOR_BADGE`].
pub const OPERATION_QUIESCE: u64 = 8;

/// **The badge on the copy of a terminal's endpoint its supervisor quiesces it through.** A
/// terminal serves [`OPERATION_QUIESCE`] on this badge and nothing else, and refuses
/// [`OPERATION_QUIESCE`] on every other.
///
/// The quiesce still rides the served endpoint, so its queue does the draining; the badge only
/// says who sent it, and a separate control endpoint would lose that ordering. Minting a badged
/// copy takes the right to grant the unbadged endpoint, which only the programs that build and
/// supervise terminals hold. No program a person runs holds it: the shell, the input driver and
/// every session hold the right to write alone. A badged copy cannot be re-badged, so a
/// [`RAW_ONLY_BADGE`] or [`STDIN_BADGE`] holder cannot reach this one either. The supervisor mints
/// it for each quiesce and hands it to the program that makes the call.
pub const SUPERVISOR_BADGE: u64 = 3;

/// **Whether a terminal serves `operation` on a capability badged `badge`**, `0` for an
/// unbadged one. Every badge is an allowlist, so an operation added to the contract later is
/// refused on it until somebody decides its holder may send it:
///
/// - unbadged, the copy the input driver, the shell and the login program hold:
///   [`OPERATION_WRITE`], [`OPERATION_READLINE`], [`OPERATION_BYTES`], [`OPERATION_INTRCOUNT`],
///   [`OPERATION_SUSPEND_COUNT`], [`OPERATION_PRINT`], [`OPERATION_RAWMODE`] and
///   [`OPERATION_READRAW`]. Not [`OPERATION_READ_LINE_BYTES`], so the shell's lines cannot be read
///   eight bytes at a time from under it, and not [`OPERATION_QUIESCE`], so none of them can stop
///   the terminal;
/// - [`RAW_ONLY_BADGE`]: [`OPERATION_RAWMODE`] and [`OPERATION_READRAW`], so nothing its holder
///   sends reaches the queue the shell reads its next command from;
/// - [`STDIN_BADGE`]: [`OPERATION_READ_LINE_BYTES`] alone;
/// - [`SUPERVISOR_BADGE`]: [`OPERATION_QUIESCE`] alone;
/// - any other badge: nothing.
///
/// `true` says only that the badge admits it; the terminal may still refuse it in its state (a
/// second parked read, a quiesce with no control endpoint).
pub const fn admits(badge: u64, operation: u64) -> bool {
    match badge {
        0 => matches!(
            operation,
            OPERATION_WRITE
                | OPERATION_READLINE
                | OPERATION_BYTES
                | OPERATION_INTRCOUNT
                | OPERATION_SUSPEND_COUNT
                | OPERATION_PRINT
                | OPERATION_RAWMODE
                | OPERATION_READRAW
        ),
        RAW_ONLY_BADGE => matches!(operation, OPERATION_RAWMODE | OPERATION_READRAW),
        STDIN_BADGE => operation == OPERATION_READ_LINE_BYTES,
        SUPERVISOR_BADGE => operation == OPERATION_QUIESCE,
        _ => false,
    }
}

/// The reply word to [`OPERATION_QUIESCE`]: the four ASCII bytes `QUIT`, read as a big-endian
/// number.
pub const QUIESCED: u64 = 0x5155_4954;

/// Supervisor → quiesced terminal, on its control endpoint: go back to serving. The swap did not
/// commit, and this instance never lost its state.
pub const CTL_RESUME: u64 = 1;
/// Supervisor → quiesced terminal, on its control endpoint: exit. The replacement has taken
/// over.
pub const CTL_QUIT: u64 = 2;

/// Supervisor → quiesced terminal, on its control endpoint: **write your state into the run this
/// message carries**, for a terminal whose handoff memory comes from a reserve the supervisor
/// holds and so cannot be mapped when the program is built. A `SEND_CAPABILITY` of the run; a
/// plain `SEND` means "into the run you already map", which is a replacement that absorbed from it
/// earlier in the same swap. Answered [`NOTE_WRITTEN`] or [`NOTE_NOT_WRITTEN`]. A replacement
/// started with [`START_ABSORB`] takes the same message, before it serves, and reads the run
/// instead.
pub const CTL_HANDOFF: u64 = 3;

/// A terminal's third start argument, a set of bits its supervisor passes. `START_HANDOFF`: a
/// handoff page is mapped at the address the terminal and its supervisor agree on, so a quiesce
/// writes the blob there. `START_ABSORB`: this instance replaces another, so it reads that blob
/// before serving and tells its supervisor [`NOTE_ABSORBED`] or [`NOTE_NOT_ABSORBED`] on the slot
/// after its control endpoint.
pub const START_HANDOFF: u64 = 1 << 0;
/// See [`START_HANDOFF`].
pub const START_ABSORB: u64 = 1 << 1;
/// Terminal → supervisor: the blob was absorbed; this instance is serving.
pub const NOTE_ABSORBED: u64 = 1;
/// Terminal → supervisor: the blob was refused; this instance is leaving without serving.
/// The second word is the refusal: 0 no blob, 1 malformed, otherwise the layout found.
pub const NOTE_NOT_ABSORBED: u64 = 2;

/// Terminal → supervisor, after [`CTL_HANDOFF`]: the blob is written. The second word is its
/// length in bytes.
pub const NOTE_WRITTEN: u64 = 3;
/// Terminal → supervisor, after [`CTL_HANDOFF`]: nothing was written, because no run came and
/// none is mapped, or the run would not map. The instance is still quiesced, and waits for
/// [`CTL_RESUME`] or [`CTL_QUIT`].
pub const NOTE_NOT_WRITTEN: u64 = 4;

/// Pack a request's first word from an opcode and a length/count.
pub const fn req(operation: u64, len: u64) -> u64 {
    (operation << OPERATION_SHIFT) | (len & 0xffff_ffff)
}
/// The opcode of a request word.
pub const fn operation(w0: u64) -> u64 {
    w0 >> OPERATION_SHIFT
}
/// The length/count of a request word.
pub const fn len(w0: u64) -> usize {
    (w0 & 0xffff_ffff) as usize
}

/// Bits 55:32 of a request's first word, which no operation gives a meaning.
const RESERVED_REQUEST_BITS: u64 = ((1 << OPERATION_SHIFT) - 1) & !0xffff_ffff;

/// **A request's first word as a server reads it**: its operation and its length or count, or
/// `None` when any reserved bit (55:32) is set. A terminal answers `None` with [`BAD_REQUEST`]
/// before it looks at the operation, so a bit nobody defined is refused rather than ignored, and a
/// later version can give it a meaning without a client already relying on it. [`req`] never sets
/// one. Every terminal server decodes through this.
///
/// ```
/// use terminal_contract::{OPERATION_WRITE, decode, req};
///
/// assert_eq!(decode(req(OPERATION_WRITE, 5)), Some((OPERATION_WRITE, 5)));
/// assert_eq!(decode(req(OPERATION_WRITE, 5) | 1 << 32), None);
/// ```
#[must_use]
pub const fn decode(w0: u64) -> Option<(u64, usize)> {
    if w0 & RESERVED_REQUEST_BITS != 0 {
        return None;
    }
    Some((operation(w0), len(w0)))
}

/// [`OPERATION_READLINE`] reply flag: end of input (^D on an empty line). The line length is 0.
pub const FLAG_EOF: u64 = 1 << 0;
/// [`OPERATION_READLINE`] reply flag: the read was interrupted (^C). The line length is 0. This is
/// the contract's hook for interrupt routing.
pub const FLAG_INTERRUPTED: u64 = 1 << 1;
/// Read reply flag: **ask again**. The terminal is about to be
/// replaced and could not hold this read across the swap; nothing was typed away. Re-issue the
/// same request, unchanged, and the terminal that answers it resumes the line where it was.
///
/// An [`OPERATION_READLINE`] reply carries it in r1 with r0 = 0. An [`OPERATION_READRAW`] reply cannot,
/// because r1 is the data there, so it is r0 = 0 (never a byte count, which is 1..=8) with this
/// flag in r1. [`is_retry`] reads both.
pub const FLAG_RETRY: u64 = 1 << 2;

/// **Whether a read reply means "ask again".** One test for both read shapes, so a reader
/// cannot check the flag in the wrong register.
pub const fn is_retry(r0: u64, r1: u64) -> bool {
    r0 == 0 && r1 == FLAG_RETRY
}

/// The reply to a request whose opcode the terminal does not implement, or whose first word sets
/// a reserved bit ([`decode`]): r0 = this, r1 = 0.
/// A sentinel rather than silence, so a confused client fails fast instead of hanging.
pub const BAD_REQUEST: u64 = u64::MAX;

/// This interface's identity in a program's `INTERFACES` note: 25, the terminal's requests and
/// replies, with no magic. The number is this interface's for good: no other interface takes it,
/// and a format change is a new version, never a new number. A program declares the pair of this
/// id and [`VERSION`]. The identity is frozen at interface 25, version 1.
pub const INTERFACE_ID: u64 = 25;

/// The wire's version, one for the operations, flags, badges and reply shapes together. A break
/// bumps it; an addition (a new operation, a new flag) does not.
pub const VERSION: u32 = 1;

#[cfg(test)]
mod tests;

#[cfg(kani)]
mod verification;
