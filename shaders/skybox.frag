#version 430 core

in vec3 f_dir;
in vec3 tex_coord;

out vec4 frag_color;

uniform samplerCube skybox;
uniform vec3 sun_color;
uniform vec3 sun_direction;
uniform float sun_size;

void main() {
    float likeness = dot(normalize(f_dir), sun_direction);
    vec3 sky_color = texture(skybox, tex_coord).xyz;
    sky_color += smoothstep(mix(1.0, 0.99, sun_size), 1.0, likeness);
    sky_color *= sun_color;

    frag_color = vec4(sky_color, 1.0);
}