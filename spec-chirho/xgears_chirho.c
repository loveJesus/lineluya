// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

#include <xcb/xcb.h>

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#define WINDOW_WIDTH_CHIRHO 400U
#define WINDOW_HEIGHT_CHIRHO 300U
#define FRAME_NS_CHIRHO 16666666ULL
#define FPS_REPORT_NS_CHIRHO 2000000000ULL
#define ROTATION_SAMPLES_CHIRHO 64
#define FIXED_SCALE_CHIRHO 1024
#define FLUSH_FRAME_GROUP_CHIRHO 256ULL
#define CONNECT_RETRIES_CHIRHO 30
#define CONNECT_RETRY_US_CHIRHO 100000U

typedef struct RotationSampleChirho {
    int cos_fixed_chirho;
    int sin_fixed_chirho;
} RotationSampleChirho;

typedef struct XgearsStateChirho {
    xcb_connection_t *connection_chirho;
    xcb_screen_t *screen_chirho;
    xcb_window_t window_chirho;
    xcb_gcontext_t gc_chirho;
    uint16_t width_chirho;
    uint16_t height_chirho;
    uint32_t background_pixel_chirho;
    uint32_t outline_pixel_chirho;
    uint32_t accent_pixel_chirho;
    uint32_t axis_pixel_chirho;
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

static void set_gc_foreground_chirho(
    XgearsStateChirho *state_chirho,
    uint32_t pixel_chirho
) {
    uint32_t values_chirho[1];

    values_chirho[0] = pixel_chirho;
    xcb_change_gc(
        state_chirho->connection_chirho,
        state_chirho->gc_chirho,
        XCB_GC_FOREGROUND,
        values_chirho
    );
}

static void rotate_point_chirho(
    int local_x_chirho,
    int local_y_chirho,
    const RotationSampleChirho *sample_chirho,
    int center_x_chirho,
    int center_y_chirho,
    xcb_point_t *point_chirho
) {
    int rotated_x_chirho;
    int rotated_y_chirho;

    rotated_x_chirho = ((local_x_chirho * sample_chirho->cos_fixed_chirho)
        - (local_y_chirho * sample_chirho->sin_fixed_chirho))
        / FIXED_SCALE_CHIRHO;
    rotated_y_chirho = ((local_x_chirho * sample_chirho->sin_fixed_chirho)
        + (local_y_chirho * sample_chirho->cos_fixed_chirho))
        / FIXED_SCALE_CHIRHO;

    point_chirho->x = (int16_t) (center_x_chirho + rotated_x_chirho);
    point_chirho->y = (int16_t) (center_y_chirho + rotated_y_chirho);
}

static void draw_rotating_rectangle_chirho(XgearsStateChirho *state_chirho) {
    const RotationSampleChirho *sample_chirho;
    xcb_point_t corners_chirho[5];
    xcb_segment_t axes_chirho[2];
    xcb_segment_t spokes_chirho[2];
    xcb_rectangle_t fill_rectangles_chirho[2];
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

    fill_rectangles_chirho[0].x = 0;
    fill_rectangles_chirho[0].y = 0;
    fill_rectangles_chirho[0].width = state_chirho->width_chirho;
    fill_rectangles_chirho[0].height = state_chirho->height_chirho;

    fill_rectangles_chirho[1].x = (int16_t) (center_x_chirho - 6);
    fill_rectangles_chirho[1].y = (int16_t) (center_y_chirho - 6);
    fill_rectangles_chirho[1].width = 12;
    fill_rectangles_chirho[1].height = 12;

    axes_chirho[0].x1 = 0;
    axes_chirho[0].y1 = (int16_t) center_y_chirho;
    axes_chirho[0].x2 = (int16_t) state_chirho->width_chirho;
    axes_chirho[0].y2 = (int16_t) center_y_chirho;
    axes_chirho[1].x1 = (int16_t) center_x_chirho;
    axes_chirho[1].y1 = 0;
    axes_chirho[1].x2 = (int16_t) center_x_chirho;
    axes_chirho[1].y2 = (int16_t) state_chirho->height_chirho;

    spokes_chirho[0].x1 = (int16_t) center_x_chirho;
    spokes_chirho[0].y1 = (int16_t) center_y_chirho;
    spokes_chirho[0].x2 = (int16_t) (center_x_chirho + ((sample_chirho->cos_fixed_chirho * spoke_length_chirho) / FIXED_SCALE_CHIRHO));
    spokes_chirho[0].y2 = (int16_t) (center_y_chirho + ((sample_chirho->sin_fixed_chirho * spoke_length_chirho) / FIXED_SCALE_CHIRHO));
    spokes_chirho[1].x1 = (int16_t) center_x_chirho;
    spokes_chirho[1].y1 = (int16_t) center_y_chirho;
    spokes_chirho[1].x2 = (int16_t) (center_x_chirho - ((sample_chirho->sin_fixed_chirho * spoke_length_chirho) / FIXED_SCALE_CHIRHO));
    spokes_chirho[1].y2 = (int16_t) (center_y_chirho + ((sample_chirho->cos_fixed_chirho * spoke_length_chirho) / FIXED_SCALE_CHIRHO));

    set_gc_foreground_chirho(state_chirho, state_chirho->background_pixel_chirho);
    xcb_poly_fill_rectangle(
        state_chirho->connection_chirho,
        state_chirho->window_chirho,
        state_chirho->gc_chirho,
        1,
        &fill_rectangles_chirho[0]
    );

    set_gc_foreground_chirho(state_chirho, state_chirho->axis_pixel_chirho);
    xcb_poly_segment(
        state_chirho->connection_chirho,
        state_chirho->window_chirho,
        state_chirho->gc_chirho,
        2,
        axes_chirho
    );

    set_gc_foreground_chirho(state_chirho, state_chirho->outline_pixel_chirho);
    xcb_poly_line(
        state_chirho->connection_chirho,
        XCB_COORD_MODE_ORIGIN,
        state_chirho->window_chirho,
        state_chirho->gc_chirho,
        5,
        corners_chirho
    );

    set_gc_foreground_chirho(state_chirho, state_chirho->accent_pixel_chirho);
    xcb_poly_segment(
        state_chirho->connection_chirho,
        state_chirho->window_chirho,
        state_chirho->gc_chirho,
        2,
        spokes_chirho
    );
    xcb_poly_fill_rectangle(
        state_chirho->connection_chirho,
        state_chirho->window_chirho,
        state_chirho->gc_chirho,
        1,
        &fill_rectangles_chirho[1]
    );

    /* Flush every 4th frame to batch X11 ops into fewer writev calls */
    if (state_chirho->total_frames_chirho % 4 == 3) {
}
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

static void flush_if_needed_chirho(XgearsStateChirho *state_chirho) {
    if (state_chirho->connection_chirho == NULL) {
        return;
    }

    if ((state_chirho->total_frames_chirho % FLUSH_FRAME_GROUP_CHIRHO) == 0) {
        xcb_flush(state_chirho->connection_chirho);
    }
}

static xcb_screen_t *find_screen_chirho(
    xcb_connection_t *connection_chirho,
    int preferred_screen_chirho
) {
    const xcb_setup_t *setup_chirho;
    xcb_screen_iterator_t iterator_chirho;
    int index_chirho;

    setup_chirho = xcb_get_setup(connection_chirho);
    if (setup_chirho == NULL) {
        return NULL;
    }

    iterator_chirho = xcb_setup_roots_iterator(setup_chirho);
    for (index_chirho = 0; iterator_chirho.rem != 0; ++index_chirho, xcb_screen_next(&iterator_chirho)) {
        if (index_chirho == preferred_screen_chirho) {
            return iterator_chirho.data;
        }
    }

    iterator_chirho = xcb_setup_roots_iterator(setup_chirho);
    return iterator_chirho.data;
}

static int init_xgears_state_chirho(XgearsStateChirho *state_chirho) {
    int retry_chirho;

    memset(state_chirho, 0, sizeof(*state_chirho));

    for (retry_chirho = 0; retry_chirho < CONNECT_RETRIES_CHIRHO; ++retry_chirho) {
        int preferred_screen_chirho = 0;

        state_chirho->connection_chirho = xcb_connect(NULL, &preferred_screen_chirho);
        if (state_chirho->connection_chirho != NULL
            && xcb_connection_has_error(state_chirho->connection_chirho) == 0) {
            state_chirho->screen_chirho = find_screen_chirho(
                state_chirho->connection_chirho,
                preferred_screen_chirho
            );
            if (state_chirho->screen_chirho != NULL) {
                break;
            }
        }

        if (state_chirho->connection_chirho != NULL) {
            xcb_disconnect(state_chirho->connection_chirho);
            state_chirho->connection_chirho = NULL;
        }
        usleep(CONNECT_RETRY_US_CHIRHO);
    }

    if (state_chirho->connection_chirho == NULL || state_chirho->screen_chirho == NULL) {
        fprintf(stderr, "xgears-chirho: failed to connect to X server after retries\n");
        return 0;
    }

    state_chirho->width_chirho = WINDOW_WIDTH_CHIRHO;
    state_chirho->height_chirho = WINDOW_HEIGHT_CHIRHO;
    state_chirho->background_pixel_chirho = state_chirho->screen_chirho->black_pixel;
    if (state_chirho->screen_chirho->root_depth >= 24) {
        state_chirho->outline_pixel_chirho = 0x0000ffffU;
        state_chirho->accent_pixel_chirho = 0x00ffa500U;
        state_chirho->axis_pixel_chirho = 0x00303030U;
    } else {
        state_chirho->outline_pixel_chirho = state_chirho->screen_chirho->white_pixel;
        state_chirho->accent_pixel_chirho = state_chirho->screen_chirho->white_pixel;
        state_chirho->axis_pixel_chirho = state_chirho->screen_chirho->white_pixel;
    }

    state_chirho->window_chirho = xcb_generate_id(state_chirho->connection_chirho);
    {
        uint32_t values_chirho[2];
        values_chirho[0] = state_chirho->background_pixel_chirho;
        values_chirho[1] = XCB_EVENT_MASK_EXPOSURE
            | XCB_EVENT_MASK_KEY_PRESS
            | XCB_EVENT_MASK_STRUCTURE_NOTIFY;
        xcb_create_window(
            state_chirho->connection_chirho,
            XCB_COPY_FROM_PARENT,
            state_chirho->window_chirho,
            state_chirho->screen_chirho->root,
            40,
            40,
            state_chirho->width_chirho,
            state_chirho->height_chirho,
            1,
            XCB_WINDOW_CLASS_INPUT_OUTPUT,
            state_chirho->screen_chirho->root_visual,
            XCB_CW_BACK_PIXEL | XCB_CW_EVENT_MASK,
            values_chirho
        );
    }

    state_chirho->gc_chirho = xcb_generate_id(state_chirho->connection_chirho);
    {
        uint32_t gc_values_chirho[2];
        gc_values_chirho[0] = state_chirho->outline_pixel_chirho;
        gc_values_chirho[1] = state_chirho->background_pixel_chirho;
        xcb_create_gc(
            state_chirho->connection_chirho,
            state_chirho->gc_chirho,
            state_chirho->window_chirho,
            XCB_GC_FOREGROUND | XCB_GC_BACKGROUND,
            gc_values_chirho
        );
    }

    {
        static const char title_chirho[] = "xgears-chirho";
        xcb_change_property(
            state_chirho->connection_chirho,
            XCB_PROP_MODE_REPLACE,
            state_chirho->window_chirho,
            XCB_ATOM_WM_NAME,
            XCB_ATOM_STRING,
            8,
            sizeof(title_chirho) - 1,
            title_chirho
        );
    }

    xcb_map_window(state_chirho->connection_chirho, state_chirho->window_chirho);
    xcb_flush(state_chirho->connection_chirho);

    state_chirho->last_report_ns_chirho = monotonic_ns_chirho();
    state_chirho->running_chirho = 1;
    return 1;
}

static void handle_event_chirho(
    XgearsStateChirho *state_chirho,
    xcb_generic_event_t *event_chirho
) {
    uint8_t response_type_chirho;

    response_type_chirho = (uint8_t) (event_chirho->response_type & 0x7f);
    switch (response_type_chirho) {
        case XCB_CONFIGURE_NOTIFY: {
            xcb_configure_notify_event_t *configure_event_chirho;
            configure_event_chirho = (xcb_configure_notify_event_t *) event_chirho;
            state_chirho->width_chirho = configure_event_chirho->width;
            state_chirho->height_chirho = configure_event_chirho->height;
            break;
        }
        case XCB_KEY_PRESS:
        case XCB_DESTROY_NOTIFY:
            state_chirho->running_chirho = 0;
            break;
        default:
            break;
    }
}

static void run_xgears_loop_chirho(XgearsStateChirho *state_chirho) {
    while (state_chirho->running_chirho) {
        xcb_generic_event_t *event_chirho;

        while ((event_chirho = xcb_poll_for_event(state_chirho->connection_chirho)) != NULL) {
            handle_event_chirho(state_chirho, event_chirho);
            free(event_chirho);
        }

        if (xcb_connection_has_error(state_chirho->connection_chirho) != 0) {
            fprintf(stderr, "xgears-chirho: XCB connection error\n");
            break;
        }

        draw_rotating_rectangle_chirho(state_chirho);
        state_chirho->phase_index_chirho = (state_chirho->phase_index_chirho + 1) % ROTATION_SAMPLES_CHIRHO;
        state_chirho->total_frames_chirho += 1;
        state_chirho->report_frames_chirho += 1;
        flush_if_needed_chirho(state_chirho);
        report_fps_if_needed_chirho(state_chirho);
        /* No sleep — draw as fast as possible for maximum frame rate */
    }
}

static void destroy_xgears_state_chirho(XgearsStateChirho *state_chirho) {
    if (state_chirho->connection_chirho == NULL) {
        return;
    }

    if (state_chirho->gc_chirho != 0) {
        xcb_free_gc(state_chirho->connection_chirho, state_chirho->gc_chirho);
        state_chirho->gc_chirho = 0;
    }

    if (state_chirho->window_chirho != 0) {
        xcb_flush(state_chirho->connection_chirho);
        xcb_destroy_window(state_chirho->connection_chirho, state_chirho->window_chirho);
        state_chirho->window_chirho = 0;
    }

    xcb_disconnect(state_chirho->connection_chirho);
    state_chirho->connection_chirho = NULL;
    state_chirho->screen_chirho = NULL;
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
