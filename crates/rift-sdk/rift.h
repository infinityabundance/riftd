#ifndef RIFT_SDK_H
#define RIFT_SDK_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct RiftHandle RiftHandle;

typedef struct {
    uint8_t bytes[32];
} PeerId;

typedef struct {
    uint8_t bytes[32];
} SessionId;

typedef enum {
    RIFT_EVENT_NONE = 0,
    RIFT_EVENT_INCOMING_CHAT = 1,
    RIFT_EVENT_INCOMING_CALL = 2,
    RIFT_EVENT_CALL_STATE_CHANGED = 3,
    RIFT_EVENT_PEER_JOINED = 4,
    RIFT_EVENT_PEER_LEFT = 5,
    RIFT_EVENT_AUDIO_LEVEL = 6
} RiftEventTag;

typedef struct {
    RiftEventTag tag;
    PeerId peer;
    SessionId session;
    float level;
    char* text;
} RiftEvent;

typedef enum {
    RIFT_OK = 0,
    RIFT_ERR_INVALID_CONFIG = 1,
    RIFT_ERR_INIT_FAILED = 2,
    RIFT_ERR_NOT_JOINED = 3,
    RIFT_ERR_OTHER = 255
} RiftErrorCode;

// SDK versioning
const char* rift_sdk_version(void);
int rift_sdk_abi_version(void);

RiftHandle* rift_init(const char* config_path, RiftErrorCode* out_error);
void rift_free(RiftHandle* handle);

int rift_join_channel(RiftHandle* handle, const char* name, const char* password, int internet);
int rift_leave_channel(RiftHandle* handle, const char* name);
int rift_send_chat(RiftHandle* handle, const char* text);
int rift_start_ptt(RiftHandle* handle);
int rift_stop_ptt(RiftHandle* handle);
int rift_set_mute(RiftHandle* handle, int muted);

SessionId rift_start_call(RiftHandle* handle, const PeerId* peer);
int rift_accept_call(RiftHandle* handle, SessionId session);
int rift_decline_call(RiftHandle* handle, SessionId session, const char* reason);
int rift_end_call(RiftHandle* handle, SessionId session);

int rift_next_event(RiftHandle* handle, RiftEvent* out_event);
void rift_event_free(RiftEvent* event);

#ifdef __cplusplus
}
#endif

#endif
