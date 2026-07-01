#pragma once
#include <cmath>

struct Vec3 {
    float x, y, z;

    Vec3 operator+(const Vec3& o) const { return {x+o.x, y+o.y, z+o.z}; }
    Vec3 operator*(float s)        const { return {x*s, y*s, z*s}; }

    static Vec3 cross(const Vec3& a, const Vec3& b) {
        return { a.y*b.z - a.z*b.y,
                 a.z*b.x - a.x*b.z,
                 a.x*b.y - a.y*b.x };
    }
};

struct Quaternion {
    float w, x, y, z;

    static Quaternion identity() { return {1,0,0,0}; }

    // Unit quaternion from axis-angle (axis must be unit length, angle in radians)
    static Quaternion axis_angle(Vec3 axis, float angle) {
        float s = sinf(angle * 0.5f);
        return { cosf(angle * 0.5f), axis.x*s, axis.y*s, axis.z*s };
    }

    // Hamilton product q * r
    Quaternion operator*(const Quaternion& r) const {
        return {
            w*r.w - x*r.x - y*r.y - z*r.z,
            w*r.x + x*r.w + y*r.z - z*r.y,
            w*r.y - x*r.z + y*r.w + z*r.x,
            w*r.z + x*r.y - y*r.x + z*r.w
        };
    }

    // Rotate vector v by this quaternion (Rodrigues formula, no inverse needed for unit q)
    Vec3 rotate(Vec3 v) const {
        Vec3 qv = {x, y, z};
        Vec3 t  = Vec3::cross(qv, v) * 2.0f;
        return { v.x + w*t.x + (qv.y*t.z - qv.z*t.y),
                 v.y + w*t.y + (qv.z*t.x - qv.x*t.z),
                 v.z + w*t.z + (qv.x*t.y - qv.y*t.x) };
    }
};
