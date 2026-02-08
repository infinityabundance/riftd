// Rift SDK C header.
// This header exposes the minimal C ABI for initializing the SDK, joining
// channels, and polling events.
#ifndef RIFT_SDK_H
#define RIFT_SDK_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Opaque handle to the SDK runtime.
typedef struct RiftHandle RiftHandle;

// Peer identifier (32 bytes).
typedef struct {
    uint8_t bytes[32];
} PeerId;

// Session identifier (32 bytes).
typedef struct {
    uint8_t bytes[32];
} SessionId;

// Event type tags for RiftEvent.
typedef enum {
    RIFT_EVENT_NONE = 0,
    RIFT_EVENT_INCOMING_CHAT = 1,
    RIFT_EVENT_INCOMING_CALL = 2,
    RIFT_EVENT_CALL_STATE_CHANGED = 3,
    RIFT_EVENT_PEER_JOINED = 4,
    RIFT_EVENT_PEER_LEFT = 5,
    RIFT_EVENT_AUDIO_LEVEL = 6
} RiftEventTag;

// Event payload returned by rift_next_event.
typedef struct {
    RiftEventTag tag;
    PeerId peer;
    SessionId session;
    float level;
    char* text;
} RiftEvent;

// Error codes returned by C API calls.
typedef enum {
    RIFT_OK = 0,
    RIFT_ERR_INVALID_CONFIG = 1,
    RIFT_ERR_INIT_FAILED = 2,
    RIFT_ERR_NOT_JOINED = 3,
    RIFT_ERR_OTHER = 255
} RiftErrorCode;

// SDK versioning
// Return the SDK version string.
const char* rift_sdk_version(void);
// Return the SDK ABI version.
int rift_sdk_abi_version(void);

// Initialize the SDK from a TOML config path (or defaults if null).
RiftHandle* rift_init(const char* config_path, RiftErrorCode* out_error);
// Free a previously allocated Rift handle.
void rift_free(RiftHandle* handle);

// Join a channel by name/password.
int rift_join_channel(RiftHandle* handle, const char* name, const char* password, int internet);
// Leave the current channel.
int rift_leave_channel(RiftHandle* handle, const char* name);
// Send a chat message to peers.
int rift_send_chat(RiftHandle* handle, const char* text);
// Enable push-to-talk.
int rift_start_ptt(RiftHandle* handle);
// Disable push-to-talk.
int rift_stop_ptt(RiftHandle* handle);
// Mute or unmute microphone capture.
int rift_set_mute(RiftHandle* handle, int muted);

// Start a call to a specific peer.
SessionId rift_start_call(RiftHandle* handle, const PeerId* peer);
// Accept an incoming call.
int rift_accept_call(RiftHandle* handle, SessionId session);
// Decline an incoming call with optional reason.
int rift_decline_call(RiftHandle* handle, SessionId session, const char* reason);
// End an active call.
int rift_end_call(RiftHandle* handle, SessionId session);

// Poll the next event (non-blocking).
int rift_next_event(RiftHandle* handle, RiftEvent* out_event);
// Free event resources (strings).
void rift_event_free(RiftEvent* event);

#ifdef __cplusplus
}
#endif

#endif
