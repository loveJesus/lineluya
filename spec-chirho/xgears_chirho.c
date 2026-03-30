// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

#include <X11/Xatom.h>
#include <X11/Xlib.h>

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#define WINDOW_WIDTH_CHIRHO 400U
#define WINDOW_HEIGHT_CHIRHO 300U
#define FRAME_NS_CHIRHO 16666666ULL
#define FPS_REPORT_NS_CHIRHO 5000000000ULL
#define ROTATION_SAMPLES_CHIRHO 64
#define FIXED_SCALE_CHIRHO 1024

typedef struct RotationSampleChirho {
    int cos_fixed_chirho;
    int sin_fixed_chirho;
} RotationSampleChirho;

typedef struct XgearsStateChirho {
    Display *display_chirho;
    int screen_chirho;
    Window window_chirho;
    GC gc_chirho;
    Atom wm_delete_atom_chirho;
    unsigned int width_chirho;
    unsigned int height_chirho;
    unsigned long background_pixel_chirho;
    unsigned long outline_pixel_chirho;
    unsigned long accent_pixel_chirho;
    unsigned long axis_pixel_chirho;
    uint64_t total_frames_chirho;
    uint64_t report_frames_chirho;
    uint64_t last_report_ns_chirho;
    int phase_index_chirho;
    int running_chirho;
} XgearsStateChirho;

static const RotationSampleChirho ROTATION_TABLE_CHIRHO[ROTATION_SAMPLES_CHIRHO] = {
    { 1024,     0},
    { 1019,   100},
    { 1004,   200},
    {  980,   297},
    {  946,   392},
    {  903,   483},
    {  851,   569},
    {  792,   650},
    {  724,   724},
    {  650,   792},
    {  569,   851},
    {  483,   903},
    {  392,   946},
    {  297,   980},
    {  200,  1004},
    {  100,  1019},
    {    0,  1024},
    { -100,  1019},
    { -200,  1004},
    { -297,   980},
    { -392,   946},
    { -483,   903},
    { -569,   851},
    { -650,   792},
    { -724,   724},
    { -792,   650},
    { -851,   569},
    { -903,   483},
    { -946,   392},
    { -980,   297},
    {-1004,   200},
    {-1019,   100},
    {-1024,     0},
    {-1019,  -100},
    {-1004,  -200},
    { -980,  -297},
    { -946,  -392},
    { -903,  -483},
    { -851,  -569},
    { -792,  -650},
    { -724,  -724},
    { -650,  -792},
    { -569,  -851},
    { -483,  -903},
    { -392,  -946},
    { -297,  -980},
    { -200, -1004},
    { -100, -1019},
    {    0, -1024},
    {  100, -1019},
    {  200, -1004},
    {  297,  -980},
    {  392,  -946},
    {  483,  -903},
    {  569,  -851},
    {  650,  -792},
    {  724,  -724},
    {  792,  -650},
    {  851,  -569},
    {  903,  -483},
    {  946,  -392},
    {  980,  -297},
    { 1004,  -200},
    { 1019,  -100},
};

static uint64_t monotonic_ns_chirho(void) {
    struct timespec timestamp_chirho;

    if (clock_gettime(CLOCK_MONOTONIC, &timestamp_chirho) != 0) {
        return 0;
    }

    return ((uint64_t) timestamp_chirho.tv_sec * 1000000000ULL)
        + (uint64_t) timestamp_chirho.tv_nsec;
}

static unsigned long alloc_named_color_chirho(
    Display *display_chirho,
    int screen_chirho,
    const char *name_chirho,
    unsigned long fallback_pixel_chirho
) {
    Colormap colormap_chirho;
    XColor exact_color_chirho;
    XColor screen_color_chirho;

    colormap_chirho = DefaultColormap(display_chirho, screen_chirho);
    if (XAllocNamedColor(
            display_chirho,
            colormap_chirho,
            name_chirho,
            &screen_color_chirho,
            &exact_color_chirho
        ) == 0) {
        return fallback_pixel_chirho;
    }

    return screen_color_chirho.pixel;
}

static void rotate_point_chirho(
    int local_x_chirho,
    int local_y_chirho,
    const RotationSampleChirho *sample_chirho,
    int center_x_chirho,
    int center_y_chirho,
    XPoint *point_chirho
) {
    int rotated_x_chirho;
    int rotated_y_chirho;

    rotated_x_chirho = ((local_x_chirho * sample_chirho->cos_fixed_chirho)
        - (local_y_chirho * sample_chirho->sin_fixed_chirho))
        / FIXED_SCALE_CHIRHO;
    rotated_y_chirho = ((local_x_chirho * sample_chirho->sin_fixed_chirho)
        + (local_y_chirho * sample_chirho->cos_fixed_chirho))
        / FIXED_SCALE_CHIRHO;

    point_chirho->x = (short) (center_x_chirho + rotated_x_chirho);
    point_chirho->y = (short) (center_y_chirho + rotated_y_chirho);
}

static void draw_rotating_rectangle_chirho(XgearsStateChirho *state_chirho) {
    const RotationSampleChirho *sample_chirho;
    XPoint corners_chirho[5];
    int center_x_chirho;
    int center_y_chirho;
    int half_width_chirho;
    int half_height_chirho;
    int spoke_length_chirho;

    sample_chirho = &ROTATION_TABLE_CHIRHO[state_chirho->phase_index_chirho % ROTATION_SAMPLES_CHIRHO];
    center_x_chirho = (int) state_chirho->width_chirho / 2;
    center_y_chirho = (int) state_chirho->height_chirho / 2;
    half_width_chirho = (int) state_chirho->width_chirho / 5;
    half_height_chirho = (int) state_chirho->height_chirho / 6;
    spoke_length_chirho = (half_width_chirho < half_height_chirho) ? half_width_chirho : half_height_chirho;

    rotate_point_chirho(-half_width_chirho, -half_height_chirho, sample_chirho, center_x_chirho, center_y_chirho, &corners_chirho[0]);
    rotate_point_chirho( half_width_chirho, -half_height_chirho, sample_chirho, center_x_chirho, center_y_chirho, &corners_chirho[1]);
    rotate_point_chirho( half_width_chirho,  half_height_chirho, sample_chirho, center_x_chirho, center_y_chirho, &corners_chirho[2]);
    rotate_point_chirho(-half_width_chirho,  half_height_chirho, sample_chirho, center_x_chirho, center_y_chirho, &corners_chirho[3]);
    corners_chirho[4] = corners_chirho[0];

    XSetForeground(state_chirho->display_chirho, state_chirho->gc_chirho, state_chirho->background_pixel_chirho);
    XFillRectangle(
        state_chirho->display_chirho,
        state_chirho->window_chirho,
        state_chirho->gc_chirho,
        0,
        0,
        state_chirho->width_chirho,
        state_chirho->height_chirho
    );

    XSetForeground(state_chirho->display_chirho, state_chirho->gc_chirho, state_chirho->axis_pixel_chirho);
    XDrawLine(
        state_chirho->display_chirho,
        state_chirho->window_chirho,
        state_chirho->gc_chirho,
        0,
        center_y_chirho,
        (int) state_chirho->width_chirho,
        center_y_chirho
    );
    XDrawLine(
        state_chirho->display_chirho,
        state_chirho->window_chirho,
        state_chirho->gc_chirho,
        center_x_chirho,
        0,
        center_x_chirho,
        (int) state_chirho->height_chirho
    );

    XSetForeground(state_chirho->display_chirho, state_chirho->gc_chirho, state_chirho->outline_pixel_chirho);
    XDrawLines(
        state_chirho->display_chirho,
        state_chirho->window_chirho,
        state_chirho->gc_chirho,
        corners_chirho,
        5,
        CoordModeOrigin
    );

    XSetForeground(state_chirho->display_chirho, state_chirho->gc_chirho, state_chirho->accent_pixel_chirho);
    XDrawLine(
        state_chirho->display_chirho,
        state_chirho->window_chirho,
        state_chirho->gc_chirho,
        center_x_chirho,
        center_y_chirho,
        center_x_chirho + ((sample_chirho->cos_fixed_chirho * spoke_length_chirho) / FIXED_SCALE_CHIRHO),
        center_y_chirho + ((sample_chirho->sin_fixed_chirho * spoke_length_chirho) / FIXED_SCALE_CHIRHO)
    );
    XDrawLine(
        state_chirho->display_chirho,
        state_chirho->window_chirho,
        state_chirho->gc_chirho,
        center_x_chirho,
        center_y_chirho,
        center_x_chirho - ((sample_chirho->sin_fixed_chirho * spoke_length_chirho) / FIXED_SCALE_CHIRHO),
        center_y_chirho + ((sample_chirho->cos_fixed_chirho * spoke_length_chirho) / FIXED_SCALE_CHIRHO)
    );
    XFillRectangle(
        state_chirho->display_chirho,
        state_chirho->window_chirho,
        state_chirho->gc_chirho,
        center_x_chirho - 6,
        center_y_chirho - 6,
        12,
        12
    );

    XFlush(state_chirho->display_chirho);
}

static void report_fps_if_needed_chirho(XgearsStateChirho *state_chirho) {
    uint64_t now_ns_chirho;
    uint64_t delta_ns_chirho;
    double delta_seconds_chirho;
    double fps_chirho;

    now_ns_chirho = monotonic_ns_chirho();
    delta_ns_chirho = now_ns_chirho - state_chirho->last_report_ns_chirho;
    if (delta_ns_chirho < FPS_REPORT_NS_CHIRHO) {
        return;
    }

    delta_seconds_chirho = (double) delta_ns_chirho / 1000000000.0;
    fps_chirho = (delta_seconds_chirho > 0.0)
        ? ((double) state_chirho->report_frames_chirho / delta_seconds_chirho)
        : 0.0;

    fprintf(
        stderr,
        "xgears-chirho: %.2f FPS (%llu frames in %.2f seconds)\n",
        fps_chirho,
        (unsigned long long) state_chirho->report_frames_chirho,
        delta_seconds_chirho
    );
    fflush(stderr);

    state_chirho->report_frames_chirho = 0;
    state_chirho->last_report_ns_chirho = now_ns_chirho;
}

static void sleep_for_next_frame_chirho(void) {
    struct timespec sleep_time_chirho;

    sleep_time_chirho.tv_sec = 0;
    sleep_time_chirho.tv_nsec = (long) FRAME_NS_CHIRHO;
    nanosleep(&sleep_time_chirho, NULL);
}

static int init_xgears_state_chirho(XgearsStateChirho *state_chirho) {
    memset(state_chirho, 0, sizeof(*state_chirho));

    /* Retry XOpenDisplay up to 30 times (Xorg might not be ready yet) */
    for (int retry_chirho = 0; retry_chirho < 30; retry_chirho++) {
        state_chirho->display_chirho = XOpenDisplay(NULL);
        if (state_chirho->display_chirho != NULL) break;
        usleep(100000); /* 100ms */
    }
    if (state_chirho->display_chirho == NULL) {
        fprintf(stderr, "xgears-chirho: failed to open X display after retries\n");
        return 0;
    }

    state_chirho->screen_chirho = DefaultScreen(state_chirho->display_chirho);
    state_chirho->width_chirho = WINDOW_WIDTH_CHIRHO;
    state_chirho->height_chirho = WINDOW_HEIGHT_CHIRHO;
    state_chirho->background_pixel_chirho = BlackPixel(state_chirho->display_chirho, state_chirho->screen_chirho);
    state_chirho->outline_pixel_chirho = alloc_named_color_chirho(
        state_chirho->display_chirho,
        state_chirho->screen_chirho,
        "cyan",
        WhitePixel(state_chirho->display_chirho, state_chirho->screen_chirho)
    );
    state_chirho->accent_pixel_chirho = alloc_named_color_chirho(
        state_chirho->display_chirho,
        state_chirho->screen_chirho,
        "orange",
        WhitePixel(state_chirho->display_chirho, state_chirho->screen_chirho)
    );
    state_chirho->axis_pixel_chirho = alloc_named_color_chirho(
        state_chirho->display_chirho,
        state_chirho->screen_chirho,
        "gray50",
        WhitePixel(state_chirho->display_chirho, state_chirho->screen_chirho)
    );

    state_chirho->window_chirho = XCreateSimpleWindow(
        state_chirho->display_chirho,
        RootWindow(state_chirho->display_chirho, state_chirho->screen_chirho),
        40,
        40,
        state_chirho->width_chirho,
        state_chirho->height_chirho,
        1,
        state_chirho->outline_pixel_chirho,
        state_chirho->background_pixel_chirho
    );
    if (state_chirho->window_chirho == 0) {
        fprintf(stderr, "xgears-chirho: failed to create X window\n");
        XCloseDisplay(state_chirho->display_chirho);
        state_chirho->display_chirho = NULL;
        return 0;
    }

    XStoreName(state_chirho->display_chirho, state_chirho->window_chirho, "xgears-chirho");
    XSelectInput(
        state_chirho->display_chirho,
        state_chirho->window_chirho,
        ExposureMask | KeyPressMask | StructureNotifyMask
    );

    state_chirho->wm_delete_atom_chirho = XInternAtom(
        state_chirho->display_chirho,
        "WM_DELETE_WINDOW",
        False
    );
    if (state_chirho->wm_delete_atom_chirho != None) {
        XSetWMProtocols(
            state_chirho->display_chirho,
            state_chirho->window_chirho,
            &state_chirho->wm_delete_atom_chirho,
            1
        );
    }

    state_chirho->gc_chirho = XCreateGC(
        state_chirho->display_chirho,
        state_chirho->window_chirho,
        0,
        NULL
    );
    if (state_chirho->gc_chirho == NULL) {
        fprintf(stderr, "xgears-chirho: failed to create graphics context\n");
        XDestroyWindow(state_chirho->display_chirho, state_chirho->window_chirho);
        XCloseDisplay(state_chirho->display_chirho);
        state_chirho->display_chirho = NULL;
        return 0;
    }

    XMapWindow(state_chirho->display_chirho, state_chirho->window_chirho);
    XFlush(state_chirho->display_chirho);

    state_chirho->last_report_ns_chirho = monotonic_ns_chirho();
    state_chirho->running_chirho = 1;
    return 1;
}

static void handle_event_chirho(XgearsStateChirho *state_chirho, const XEvent *event_chirho) {
    switch (event_chirho->type) {
        case ConfigureNotify:
            state_chirho->width_chirho = (unsigned int) event_chirho->xconfigure.width;
            state_chirho->height_chirho = (unsigned int) event_chirho->xconfigure.height;
            break;
        case ClientMessage:
            if ((Atom) event_chirho->xclient.data.l[0] == state_chirho->wm_delete_atom_chirho) {
                state_chirho->running_chirho = 0;
            }
            break;
        case KeyPress:
            state_chirho->running_chirho = 0;
            break;
        default:
            break;
    }
}

static void run_xgears_loop_chirho(XgearsStateChirho *state_chirho) {
    while (state_chirho->running_chirho) {
        while (XPending(state_chirho->display_chirho) > 0) {
            XEvent event_chirho;
            XNextEvent(state_chirho->display_chirho, &event_chirho);
            handle_event_chirho(state_chirho, &event_chirho);
        }

        draw_rotating_rectangle_chirho(state_chirho);
        state_chirho->phase_index_chirho = (state_chirho->phase_index_chirho + 1) % ROTATION_SAMPLES_CHIRHO;
        state_chirho->total_frames_chirho += 1;
        state_chirho->report_frames_chirho += 1;
        report_fps_if_needed_chirho(state_chirho);
        sleep_for_next_frame_chirho();
    }
}

static void destroy_xgears_state_chirho(XgearsStateChirho *state_chirho) {
    if (state_chirho->display_chirho == NULL) {
        return;
    }

    if (state_chirho->gc_chirho != NULL) {
        XFreeGC(state_chirho->display_chirho, state_chirho->gc_chirho);
        state_chirho->gc_chirho = NULL;
    }

    if (state_chirho->window_chirho != 0) {
        XDestroyWindow(state_chirho->display_chirho, state_chirho->window_chirho);
        state_chirho->window_chirho = 0;
    }

    XCloseDisplay(state_chirho->display_chirho);
    state_chirho->display_chirho = NULL;
}

int main(void) {
    XgearsStateChirho state_chirho;

    if (!init_xgears_state_chirho(&state_chirho)) {
        return 1;
    }

    run_xgears_loop_chirho(&state_chirho);
    destroy_xgears_state_chirho(&state_chirho);
    return 0;
}
