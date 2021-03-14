#version 330 core
in vec3 tex_coord;

uniform samplerCube skybox;
uniform vec3 sun_color;

void main() {
    gl_FragColor = vec4(sun_color * texture(skybox, tex_coord).xyz, 1.0);
}