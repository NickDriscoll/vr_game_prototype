#version 430 core

in vec4 f_pos;
in vec4 f_color;
in vec4 f_normal;
in vec3 view_direction;
in float f_highlighted;

out vec4 frag_color;

uniform float current_time;
uniform vec3 view_position;

void main() {
    vec3 view_dir = normalize(view_position - vec3(f_pos));
    vec3 world_normal = normalize(vec3(f_normal));

    //Rim-lighting if this one is highlighted
    vec3 rim_light = vec3(0.0);
    if (f_highlighted != 0.0) {
        float likeness = 1.0 - max(0.0, dot(view_dir, world_normal));
        float factor = smoothstep(0.5, 1.0, likeness);
        vec3 color = vec3(cos(5.0 * current_time) * 0.5 + 0.5, sin(6.0 * current_time) * 0.5 + 0.5, sin(8.0 * current_time) * 0.5 + 0.5);
        rim_light = factor * color;
    }

    frag_color = f_color + vec4(rim_light, 0.0);
}