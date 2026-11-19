// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// OpenCabinet's 3D view: surfaces shaded by what they are, with grain,
// plies and floor planks made here rather than read from images; their
// edges as lines; a sky behind everything. Colors are linear: the target
// turns them into sRGB.

struct Globals {
    view_projection: mat4x4<f32>,
    eye: vec4<f32>,
    // x: 1 to draw textures; y: how opaque seen-through surfaces are.
    options: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;

struct SurfaceIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) params: vec4<f32>,
};

struct SurfaceOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) params: vec4<f32>,
};

@vertex
fn surface_vertex(v: SurfaceIn) -> SurfaceOut {
    var out: SurfaceOut;
    out.clip = g.view_projection * vec4<f32>(v.position, 1.0);
    out.world = v.position;
    out.normal = v.normal;
    out.uv = v.uv;
    out.color = v.color;
    out.params = v.params;
    return out;
}

// A hash without sine, steady on every GPU.
fn hash(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash(i);
    let b = hash(i + vec2<f32>(1.0, 0.0));
    let c = hash(i + vec2<f32>(0.0, 1.0));
    let d = hash(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var value = 0.0;
    var amplitude = 0.5;
    var q = p;
    for (var i = 0; i < 4; i = i + 1) {
        value = value + amplitude * noise(q);
        q = q * 2.03 + vec2<f32>(17.0, 9.0);
        amplitude = amplitude * 0.5;
    }
    return value;
}

// Wood: growth rings bent by slow noise, fine pores along the grain.
// `uv` is in millimeters, the grain along x.
fn wood(uv: vec2<f32>, base: vec3<f32>, seed: f32) -> vec3<f32> {
    let s = vec2<f32>(seed * 3.1, seed * 1.7);
    let bend = fbm(vec2<f32>(uv.x * 0.0018, uv.y * 0.010) + s) * 3.0;
    let rings = fract(uv.y * 0.045 + bend);
    let late = smoothstep(0.62, 0.98, rings) * (0.6 + 0.4 * noise(uv * 0.01 + s));
    let pores = noise(vec2<f32>(uv.x * 0.05, uv.y * 2.2) + s);
    let figure = fbm(vec2<f32>(uv.x * 0.004, uv.y * 0.02) + s * 2.0);
    var c = base * (0.90 + 0.18 * figure);
    c = c * (1.0 - 0.20 * late);
    c = c * (0.95 + 0.08 * pores);
    return c;
}

// Plies across the thickness: alternate veneers, darker glue lines.
fn plies(uv: vec2<f32>, base: vec3<f32>) -> vec3<f32> {
    let ply = uv.y / 2.55;
    let index = floor(ply);
    let within = fract(ply);
    let alternate = select(1.0, 0.86, (index % 2.0) == 1.0);
    let glue = smoothstep(0.0, 0.08, within) * smoothstep(1.0, 0.92, within);
    return base * alternate * (0.72 + 0.28 * glue);
}

// Planks 180 mm wide and 1200 mm long, their joints staggered.
fn planks(uv: vec2<f32>, base: vec3<f32>) -> vec3<f32> {
    let row = floor(uv.y / 180.0);
    let shifted = uv.x + hash(vec2<f32>(row, 3.0)) * 1200.0;
    let column = floor(shifted / 1200.0);
    let local = vec2<f32>(fract(shifted / 1200.0) * 1200.0, fract(uv.y / 180.0) * 180.0);
    let tint = 0.85 + 0.25 * hash(vec2<f32>(row, column));
    var c = wood(vec2<f32>(local.x, local.y + row * 37.0), base * tint, hash(vec2<f32>(column, row)) * 40.0);
    let joint = min(min(local.x, 1200.0 - local.x), min(local.y, 180.0 - local.y));
    c = c * mix(0.55, 1.0, smoothstep(0.0, 2.0, joint));
    return c;
}

// The ground under a product alone: a light grid every 100 mm and a
// stronger one every meter, fading with distance.
fn ground(world: vec3<f32>, base: vec3<f32>) -> vec3<f32> {
    let p = world.xz;
    let fine = abs(fract(p / 100.0 + 0.5) - 0.5) * 100.0;
    let coarse = abs(fract(p / 1000.0 + 0.5) - 0.5) * 1000.0;
    let width = max(length(world - g.eye.xyz) * 0.0012, 0.6);
    let line_fine = 1.0 - smoothstep(0.0, width, min(fine.x, fine.y));
    let line_coarse = 1.0 - smoothstep(0.0, width * 1.6, min(coarse.x, coarse.y));
    let fade = 1.0 - smoothstep(2000.0, 9000.0, length(world - g.eye.xyz));
    return base * (1.0 - (0.06 * line_fine + 0.14 * line_coarse) * fade);
}

@fragment
fn surface_fragment(in: SurfaceOut) -> @location(0) vec4<f32> {
    let kind = u32(in.params.x + 0.5);
    let seed = in.params.y;
    let flags = u32(in.params.z + 0.5);
    let textured = g.options.x > 0.5;
    var albedo = in.color.rgb;
    var alpha = in.color.a;
    var shine = 0.0;
    var gloss = 24.0;

    if (kind == 1u && textured) {
        albedo = wood(in.uv, albedo, seed);
        shine = 0.10;
    } else if (kind == 2u) {
        albedo = select(albedo, planks(in.uv, albedo), textured);
        shine = 0.18;
    } else if (kind == 3u && textured) {
        albedo = albedo * (0.97 + 0.03 * noise(in.world.xy * 0.05 + in.world.zy * 0.05));
    } else if (kind == 0u) {
        shine = 0.22;
        gloss = 40.0;
    } else if (kind == 5u) {
        // Darker toward the middle of a hole, as if it went deep.
        albedo = albedo * (0.4 + 0.6 * in.uv.x);
    } else if (kind == 6u) {
        shine = 0.65;
        gloss = 60.0;
    } else if (kind == 7u && textured) {
        albedo = plies(in.uv, albedo);
    } else if (kind == 8u) {
        albedo = ground(in.world, albedo);
    } else if (kind == 9u) {
        // A soft shadow, darkest under the product.
        let edge = max(abs(in.uv.x), abs(in.uv.y));
        alpha = alpha * (1.0 - smoothstep(0.55, 1.0, edge));
        return vec4<f32>(0.0, 0.0, 0.0, alpha);
    }

    var n = normalize(in.normal);
    let to_eye = normalize(g.eye.xyz - in.world);
    if (dot(n, to_eye) < 0.0) {
        n = -n;
    }
    let key = normalize(vec3<f32>(-0.45, 0.85, 0.55));
    let fill = normalize(vec3<f32>(0.6, 0.35, -0.5));
    let sky = mix(vec3<f32>(0.30, 0.28, 0.26), vec3<f32>(0.62, 0.66, 0.74), n.y * 0.5 + 0.5);
    let light = sky * 0.85
        + vec3<f32>(1.0, 0.96, 0.88) * 0.80 * max(dot(n, key), 0.0)
        + vec3<f32>(0.75, 0.82, 1.0) * 0.22 * max(dot(n, fill), 0.0);
    var color = albedo * light;
    let half_way = normalize(key + to_eye);
    color = color + vec3<f32>(1.0, 0.97, 0.92) * shine * pow(max(dot(n, half_way), 0.0), gloss);

    if ((flags & 1u) != 0u) {
        let rim = pow(1.0 - max(dot(n, to_eye), 0.0), 2.0);
        color = mix(color, vec3<f32>(0.10, 0.42, 1.0), 0.30) + vec3<f32>(0.15, 0.35, 0.9) * rim * 0.6;
    }
    if ((flags & 2u) != 0u) {
        alpha = alpha * g.options.y;
    }
    // A gentle curve keeps highlights from clipping.
    color = color / (1.0 + color * 0.12);
    return vec4<f32>(color, alpha);
}

struct LineIn {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
};

struct LineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn line_vertex(v: LineIn) -> LineOut {
    var out: LineOut;
    out.clip = g.view_projection * vec4<f32>(v.position, 1.0);
    // Just in front of the faces they edge.
    out.clip.z = out.clip.z - 0.00004 * out.clip.w;
    out.color = v.color;
    return out;
}

@fragment
fn line_fragment(in: LineOut) -> @location(0) vec4<f32> {
    return in.color;
}

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) height: f32,
};

// One triangle that covers the view.
@vertex
fn sky_vertex(@builtin(vertex_index) i: u32) -> SkyOut {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var out: SkyOut;
    out.clip = vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 1.0, 1.0);
    out.height = y;
    return out;
}

@fragment
fn sky_fragment(in: SkyOut) -> @location(0) vec4<f32> {
    let top = vec3<f32>(0.42, 0.52, 0.66);
    let bottom = vec3<f32>(0.86, 0.85, 0.83);
    let t = clamp(in.height, 0.0, 1.0);
    return vec4<f32>(mix(bottom, top, t), 1.0);
}
