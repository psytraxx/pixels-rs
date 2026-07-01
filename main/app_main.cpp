#include <cstdio>
#include <cstring>
#include <cmath>
#include <algorithm>

#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "esp_log.h"
#include "esp_timer.h"

#include "config.h"
#include "math3d.h"
#include "display.h"
#include "touch.h"

static const char *TAG = "app";

// ---------------------------------------------------------------------------
// 3-D rendering constants
// ---------------------------------------------------------------------------
static constexpr float FOV                  = 200.0f;
static constexpr float PROJECTION_DISTANCE  = 4.0f;
static constexpr float ROTATION_SPEED       = 0.03f;   // rad / frame (auto)
static constexpr float ROTATION_SENSITIVITY = 0.0005f; // rad / pixel (touch)

// ---------------------------------------------------------------------------
// Particle system
// ---------------------------------------------------------------------------
static constexpr int   MAX_PARTICLES  = 200;
static constexpr int   EMISSION_RATE  = 3;    // particles / frame
static constexpr float PARTICLE_SPEED = 0.02f;

struct Particle {
    Vec3     pos;
    Vec3     vel;
    uint16_t color;
    bool     active;
};

// ---------------------------------------------------------------------------
// Cube geometry
// ---------------------------------------------------------------------------
static const Vec3 CUBE_VERTS[8] = {
    {-1,-1,-1}, { 1,-1,-1}, { 1, 1,-1}, {-1, 1,-1},
    {-1,-1, 1}, { 1,-1, 1}, { 1, 1, 1}, {-1, 1, 1},
};
static const int CUBE_EDGES[12][2] = {
    {0,1},{1,2},{2,3},{3,0}, // back face
    {4,5},{5,6},{6,7},{7,4}, // front face
    {0,4},{1,5},{2,6},{3,7}, // connecting edges
};

// Particle colors (byte-swapped BE RGB565)
static const uint16_t PARTICLE_COLORS[6] = {
    Color::RED, Color::GREEN, Color::BLUE,
    Color::YELLOW, Color::CYAN, Color::MAGENTA,
};

// ---------------------------------------------------------------------------
// Simple pseudo-random from millisecond timestamp
// ---------------------------------------------------------------------------
static float frand(float t, float seed)
{
    float v = fmodf(t * seed, 1.0f);
    if (v < 0) v += 1.0f;
    return v * 2.0f - 1.0f; // [-1, 1]
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------
extern "C" void app_main()
{
    ESP_LOGI(TAG, "pixels starting");

    static Display disp;
    static TouchController touch;

    if (!disp.init()) {
        ESP_LOGE(TAG, "Display init failed – halting");
        for (;;) vTaskDelay(portMAX_DELAY);
    }
    if (!touch.init()) {
        ESP_LOGW(TAG, "Touch init failed – continuing without touch");
    }

    // --- Particle pool (stack-allocated) ------------------------------------
    static Particle particles[MAX_PARTICLES] = {};

    // --- Rotation state -----------------------------------------------------
    Quaternion rotation = Quaternion::identity();
    // Pre-compute the per-frame automatic rotation around Y-axis
    const Quaternion q_auto = Quaternion::axis_angle({0,1,0}, ROTATION_SPEED);

    // --- Touch state --------------------------------------------------------
    int16_t initial_touch_x = 0;
    int16_t initial_touch_y = 0;

    // --- FPS text buffer ----------------------------------------------------
    char fps_buf[16];

    int64_t last_us    = esp_timer_get_time();
    int64_t current_us = last_us;

    const int half_w = DISPLAY_WIDTH  / 2;
    const int half_h = DISPLAY_HEIGHT / 2;

    for (;;) {
        current_us = esp_timer_get_time();

        // ---- 1. Selective clear ----------------------------------------
        disp.clear_buffer();

        // ---- 2. Touch input --------------------------------------------
        TouchEvent te = {};
        if (touch.read(te)) {
            if (te.type == TouchEvent::Type::Down) {
                initial_touch_x = te.x;
                initial_touch_y = te.y;
            } else if (te.type == TouchEvent::Type::Up) {
                float angle_y =  (te.x - initial_touch_x) * ROTATION_SENSITIVITY;
                float angle_x = -(te.y - initial_touch_y) * ROTATION_SENSITIVITY;
                Quaternion qx = Quaternion::axis_angle({1,0,0}, angle_x);
                Quaternion qy = Quaternion::axis_angle({0,1,0}, angle_y);
                rotation = qy * qx * rotation;
            }
        }

        // ---- 3. Automatic rotation -------------------------------------
        rotation = q_auto * rotation;

        // ---- 4. Emit particles -----------------------------------------
        int64_t t_ms = current_us / 1000;
        float   t_f  = (float)t_ms;

        for (int i = 0; i < EMISSION_RATE; i++) {
            for (int p = 0; p < MAX_PARTICLES; p++) {
                if (!particles[p].active) {
                    float rx = frand(t_f, 0.123f + i * 0.037f);
                    float ry = frand(t_f, 0.456f + i * 0.073f);
                    float rz = frand(t_f, 0.789f + i * 0.011f);
                    float len = sqrtf(rx*rx + ry*ry + rz*rz);
                    if (len < 0.01f) len = 1.0f;
                    particles[p].pos    = {0,0,0};
                    particles[p].vel    = { rx/len*PARTICLE_SPEED,
                                            ry/len*PARTICLE_SPEED,
                                            rz/len*PARTICLE_SPEED };
                    int ci = (int)(fmodf(t_f * 0.321f, 6.0f));
                    if (ci < 0 || ci >= 6) ci = 0;
                    particles[p].color  = PARTICLE_COLORS[ci];
                    particles[p].active = true;
                    break;
                }
            }
        }

        // ---- 5. Update particles ---------------------------------------
        for (int p = 0; p < MAX_PARTICLES; p++) {
            if (!particles[p].active) continue;
            Vec3 &pos = particles[p].pos;
            Vec3 &vel = particles[p].vel;
            pos.x += vel.x;  pos.y += vel.y;  pos.z += vel.z;
            auto bounce = [](float &v, float &dv) {
                if (v > 1.0f)  { dv = -fabsf(dv); v =  1.0f; }
                if (v < -1.0f) { dv =  fabsf(dv); v = -1.0f; }
            };
            bounce(pos.x, vel.x);
            bounce(pos.y, vel.y);
            bounce(pos.z, vel.z);
        }

        // ---- 6. Project cube vertices ----------------------------------
        int proj_x[8], proj_y[8];
        for (int i = 0; i < 8; i++) {
            Vec3 rv = rotation.rotate(CUBE_VERTS[i]);
            float z = rv.z + PROJECTION_DISTANCE;
            if (fabsf(z) > 0.01f) {
                float inv_z = 1.0f / z;
                proj_x[i] = (int)(rv.x * FOV * inv_z) + half_w;
                proj_y[i] = (int)(rv.y * FOV * inv_z) + half_h;
            } else {
                proj_x[i] = proj_y[i] = INT32_MAX;
            }
        }

        // ---- 7. Draw cube edges ----------------------------------------
        for (int e = 0; e < 12; e++) {
            int a = CUBE_EDGES[e][0], b = CUBE_EDGES[e][1];
            if (proj_x[a] == INT32_MAX || proj_x[b] == INT32_MAX) continue;
            disp.draw_line(proj_x[a], proj_y[a], proj_x[b], proj_y[b], Color::WHITE);
        }

        // ---- 8. Draw particles -----------------------------------------
        for (int p = 0; p < MAX_PARTICLES; p++) {
            if (!particles[p].active) continue;
            Vec3 rv = rotation.rotate(particles[p].pos);
            float z = rv.z + PROJECTION_DISTANCE;
            if (fabsf(z) < 0.01f) continue;
            float inv_z = 1.0f / z;
            int px = (int)(rv.x * FOV * inv_z) + half_w;
            int py = (int)(rv.y * FOV * inv_z) + half_h;
            if (px >= 1 && px < DISPLAY_WIDTH - 1 && py >= 1 && py < DISPLAY_HEIGHT - 1)
                disp.draw_rect3x3(px, py, particles[p].color);
        }

        // ---- 9. FPS counter --------------------------------------------
        int64_t frame_us = current_us - last_us;
        if (frame_us > 0) {
            int fps = (int)(1000000LL / frame_us);
            snprintf(fps_buf, sizeof(fps_buf), "FPS:%d", fps);
            disp.draw_string(0, 0, fps_buf);
        }
        last_us = current_us;

        // ---- 10. Flush dirty tiles to display (DMA) --------------------
        disp.flush();
    }
}
