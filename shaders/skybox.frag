#version 430 core

in vec3 f_dir;
in vec3 tex_coord;

uniform samplerCube skybox;
uniform vec3 sun_color;
uniform vec3 sun_direction;

void main() {
    vec3 sky_color = sun_color * texture(skybox, tex_coord).xyz;

    float likeness = dot(normalize(f_dir), sun_direction);
    sky_color += sun_color * smoothstep(0.99, 1.0, likeness);

    gl_FragColor = vec4(sky_color, 1.0);
}