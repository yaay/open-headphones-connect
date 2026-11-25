// The shared session (core/src/session.rs) for the Mac app's Swift.
//
// Swift fills in an OhcBluetooth and calls ohc_start once. The session then runs on a thread
// of its own and calls those functions from it, waiting for each answer; it reports through
// on_snapshot and takes orders through ohc_send.
//
// A `char *` that a function of OhcBluetooth returns is made with strdup and freed by the
// session. Where it is an error message, NULL means it worked.

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef struct OhcBluetooth {
    bool (*radio_is_on)(void);

    // A paired headset whose name ohc_is_headset_name accepts, one that is connected before one
    // that is not, or NULL. The session hands it back to headset_release when done with it.
    void *(*find_headset)(void);
    char *(*headset_name)(void *headset);
    // Asked about once a second.
    bool (*headset_is_connected)(void *headset);
    // Opens the control channel, and says which protocol it speaks: Sony's first or second
    // generation (1 or 2), or Nothing's (3).
    // From then until link_close, what arrives goes to ohc_link_received and the end of the
    // channel to ohc_link_closed, both with this `inbox`.
    char *(*headset_open)(void *headset, const void *inbox, void **link, uint8_t *generation);
    // Return once the request is made; the session watches headset_is_connected after that.
    char *(*headset_connect_audio)(void *headset);
    char *(*headset_disconnect_audio)(void *headset);
    void (*headset_release)(void *headset);

    char *(*link_send)(void *link, const uint8_t *bytes, size_t length);
    // Closes the channel and lets go of `link`. Once this returns, nothing more may be given
    // to the link's inbox.
    void (*link_close)(void *link);

    // The snapshot as JSON, in the shape of src/types.ts, every time it changes. Called on the
    // session's thread; the text is gone when this returns.
    void (*on_snapshot)(const char *json);
} OhcBluetooth;

// Starts the session. Only the first call to this or ohc_start_demo has an effect.
void ohc_start(const OhcBluetooth *bluetooth);

// Starts a made-up headset of the given model in place of the session, for trying the app
// without headphones. It touches no Bluetooth; commands change the made-up state.
void ohc_start_demo(const char *model, void (*on_snapshot)(const char *json));

// Passes a command on to the session: "connect", {"set_volume": 12} and so on, the JSON form
// of `Command` in core/src/session.rs. False if it is not one, or the session has not started.
bool ohc_send(const char *command_json);

// Whether a Bluetooth device of this name is a Sony, Nothing or CMF headset.
bool ohc_is_headset_name(const char *name);

// Bytes that arrived on a control channel, in order. Always from the same thread.
void ohc_link_received(const void *inbox, const uint8_t *bytes, size_t length);
// The channel ended; `reason` is NULL for a clean close. At most once per channel.
void ohc_link_closed(const void *inbox, const char *reason);
