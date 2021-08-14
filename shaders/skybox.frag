#version 430 core
in vec3 tex_coord;

uniform samplerCube skybox;
uniform vec3 sun_color;
uniform vec3 sun_direction;

void main() {
    vec3 sky_color = sun_color * texture(skybox, tex_coord).xyz;
    gl_FragColor = vec4(sky_color, 1.0);
    //vec3 tex_coord_renorm = (tex_coord + 1.0) / 2.0;
    //gl_FragColor = vec4(tex_coord_renorm, 1.0);
}