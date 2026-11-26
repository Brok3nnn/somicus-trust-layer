//! OS-level input classification distinguishing physical HID from injected events.
//!
//! Windows:
//! - Inspects `MSLLHOOKSTRUCT.flags` and `KBDLLHOOKSTRUCT.flags`.
//! - Flags `LLMHF_INJECTED (0x01)` and `LLMHF_LOWER_IL_INJECTED (0x02)` denote synthetic mouse events.
//! - Flag `LLKHF_INJECTED (0x10)` and `LLKHF_LOWER_IL_INJECTED (0x02)` denote synthetic keyboard events.
//!
//! macOS:
//! - Inspects `CGEventGetSourceStateID(event)` and `CGEventGetIntegerValueField(event, kCGEventSourceUserData)`.
//! - `kCGEventSourceStateHIDSystemState (1)` denotes genuine physical hardware HID events.
//! - `kCGEventSourceStateCombinedSessionState (0)` or synthetic process tags denote injected events.
//!
//! Linux:
//! - Inspects `XInput2` device identity vs `XTest` / libei virtual device.

use crate::input::InputSource;

// -----------------------------------------------------------------------------
// Windows Hook Flag Constants
// -----------------------------------------------------------------------------

/// Windows `LowLevelHooks`: mouse event was injected.
pub const LLMHF_INJECTED: u32 = 0x0000_0001;

/// Windows `LowLevelHooks`: mouse event was injected from a lower integrity level process.
pub const LLMHF_LOWER_IL_INJECTED: u32 = 0x0000_0002;

/// Windows `LowLevelHooks`: keyboard event was injected.
pub const LLKHF_INJECTED: u32 = 0x0000_0010;

/// Windows `LowLevelHooks`: keyboard event was injected from a lower integrity level process.
pub const LLKHF_LOWER_IL_INJECTED: u32 = 0x0000_0002;

/// Classifies Windows low-level mouse hook flags as physical or injected.
#[must_use]
pub fn classify_windows_mouse_flags(flags: u32) -> InputSource {
    let injected_mask = LLMHF_INJECTED | LLMHF_LOWER_IL_INJECTED;
    if (flags & injected_mask) != 0 {
        InputSource::Injected { flags }
    } else {
        InputSource::Physical
    }
}

/// Classifies Windows low-level keyboard hook flags as physical or injected.
#[must_use]
pub fn classify_windows_keyboard_flags(flags: u32) -> InputSource {
    let injected_mask = LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED;
    if (flags & injected_mask) != 0 {
        InputSource::Injected { flags }
    } else {
        InputSource::Physical
    }
}

// -----------------------------------------------------------------------------
// macOS Event Source Constants
// -----------------------------------------------------------------------------

/// macOS CoreGraphics: Combined session state ID (synthetic or process-posted).
pub const K_CG_EVENT_SOURCE_STATE_COMBINED_SESSION_STATE: i32 = 0;

/// macOS CoreGraphics: Hardware HID system state ID (genuine physical input).
pub const K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE: i32 = 1;

/// Somicus synthetic actuator marker tag stored in `kCGEventSourceUserData`.
pub const SOMICUS_SYNTHETIC_USER_DATA: i64 = 0x534F_4D49_4B55_5321; // "SOMICUS!"

/// Classifies macOS `CGEvent` source state and user data fields as physical or injected.
#[must_use]
pub fn classify_macos_event_source(source_state_id: i32, user_data: i64) -> InputSource {
    if user_data == SOMICUS_SYNTHETIC_USER_DATA {
        return InputSource::Injected { flags: 0x02 };
    }

    if source_state_id == K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE {
        InputSource::Physical
    } else {
        let flags = source_state_id.unsigned_abs();
        InputSource::Injected { flags }
    }
}

// -----------------------------------------------------------------------------
// Linux Input Classification
// -----------------------------------------------------------------------------

/// Classifies Linux input event device provenance.
#[must_use]
pub fn classify_linux_device(is_virtual: bool, is_xtest_or_libei: bool) -> InputSource {
    if is_virtual || is_xtest_or_libei {
        InputSource::Injected { flags: 1 }
    } else {
        InputSource::Physical
    }
}
