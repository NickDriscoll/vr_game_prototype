use std::fs::File;
use std::io::Read;
use ozy::io;

#[derive(Debug)]
pub struct LineSegment {
    pub p0: glm::TVec4<f32>,
    pub p1: glm::TVec4<f32>
}

impl LineSegment {
    pub fn zero() -> Self {
        LineSegment {
            p0: glm::zero(),
            p1: glm::zero()
        }
    }

    pub fn length(&self) -> f32 {
        f32::sqrt(f32::powi(self.p1.x - self.p0.x, 2) + f32::powi(self.p1.y - self.p0.y, 2))
    }
}

//An infinite plane as defined by a point on the plane and a vector normal to the plane
pub struct Plane {
    pub point: glm::TVec4<f32>,
    pub normal: glm::TVec4<f32>,
}

impl Plane {
    pub fn new(point: glm::TVec4<f32>, normal: glm::TVec4<f32>) -> Self {
        Plane {
            point,
            normal
        }
    }
}

pub struct PlaneBoundaries {
    pub xmin: f32,
    pub xmax: f32,
    pub ymin: f32,
    pub ymax: f32
}

//Axis-aligned bounding box
pub struct AABB {
    pub position: glm::TVec4<f32>,
    pub width: f32,
    pub depth: f32,
    pub height: f32
}

#[derive(Debug)]
pub struct Terrain {
    pub vertices: Vec<glm::TVec3<f32>>,
    pub indices: Vec<u16>,
    pub face_normals: Vec<glm::TVec3<f32>>
}

impl Terrain {
    pub fn from_ozt(path: &str) -> Self {
        let mut terrain_file = match File::open(path) {
            Ok(file) => { file }
            Err(e) => {
                panic!("Error reading terrain file: {}", e);
            }
        };

        let vertices = {
            let byte_count = match io::read_u32(&mut terrain_file, "Error reading byte_count.") {
                Some(count) => { count as usize }
                None => { panic!("Couldn't read byte count"); }
            };

            let mut bytes = vec![0; byte_count];
            if let Err(e) = terrain_file.read_exact(bytes.as_mut_slice()) {
                panic!("Error reading vertex data from file: {}", e);
            }

            let byte_step = 12; // One f32 for each of x,y,z
            let mut vertices = Vec::with_capacity(byte_count / byte_step);            
            for i in (0..bytes.len()).step_by(byte_step) {
                let x_bytes = [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]];
                let y_bytes = [bytes[i + 4], bytes[i + 5], bytes[i + 6], bytes[i + 7]];
                let z_bytes = [bytes[i + 8], bytes[i + 9], bytes[i + 10], bytes[i + 11]];

                let x = f32::from_le_bytes(x_bytes);
                let y = f32::from_le_bytes(y_bytes);
                let z = f32::from_le_bytes(z_bytes);

                vertices.push(glm::vec3(x, y, z));
            }
            vertices
        };
        
        let indices = {
            let index_count = match io::read_u32(&mut terrain_file, "Error reading index_count") {
                Some(n) => { (n / 2) as usize }
                None => { panic!("Couldn't read byte count"); }
            };
            
            let indices = match io::read_u16_data(&mut terrain_file, index_count) {
                Some(n) => { n }
                None => { panic!("Couldn't read byte count"); }
            };
            indices
        };

        let face_normals = {
            let byte_count = match io::read_u32(&mut terrain_file, "Error reading byte_count") {
                Some(n) => { n as usize }
                None => { panic!("Couldn't read byte count"); }
            };

            let mut bytes = vec![0; byte_count];
            if let Err(e) = terrain_file.read_exact(bytes.as_mut_slice()) {
                panic!("Error reading face normal data from file: {}", e);
            }

            let byte_step = 12;
            let mut normals = Vec::with_capacity(byte_count / byte_step);            
            for i in (0..bytes.len()).step_by(byte_step) {
                let x_bytes = [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]];
                let y_bytes = [bytes[i + 4], bytes[i + 5], bytes[i + 6], bytes[i + 7]];
                let z_bytes = [bytes[i + 8], bytes[i + 9], bytes[i + 10], bytes[i + 11]];

                let x = f32::from_le_bytes(x_bytes);
                let y = f32::from_le_bytes(y_bytes);
                let z = f32::from_le_bytes(z_bytes);

                normals.push(glm::vec3(x, y, z));
            }
            normals
        };

        Self {
            vertices,
            indices,
            face_normals
        }
    }
}

pub fn segment_intersect_plane(plane: &Plane, segment: &LineSegment) -> Option<glm::TVec4<f32>> {
    let denominator = glm::dot(&plane.normal, &(segment.p1 - segment.p0));

    //Check for divide-by-zero
    if denominator != 0.0 {
        let x = glm::dot(&plane.normal, &(plane.point - segment.p0)) / denominator;
        if x > 0.0 && x <= 1.0 {
            let result = (1.0 - x) * segment.p0 + x * segment.p1;
            Some(glm::vec4(result.x, result.y, result.z, 1.0))
        } else {
            None
        }        
    } else {
        None
    }
}

pub fn standing_on_plane(plane: &Plane, segment: &LineSegment, boundaries: &PlaneBoundaries) -> Option<glm::TVec4<f32>> {
    let collision_point = segment_intersect_plane(&plane, &segment);
    if let Some(point) = collision_point {
        let on_aabb = point.x > boundaries.xmin &&
                      point.x < boundaries.xmax &&
                      point.y > boundaries.ymin &&
                      point.y < boundaries.ymax;

        if on_aabb {
            Some(point)
        } else {
            None
        }
    } else {
        None
    }
}

pub fn point_plane_distance(point: &glm::TVec4<f32>, plane: &Plane) -> f32 {
    glm::dot(&plane.normal, &(point - plane.point))
}

pub fn sign(test: &glm::TVec2<f32>, p0: &glm::TVec2<f32>, p1: &glm::TVec2<f32>) -> f32 {
    (test.x - p1.x) * (p0.y - p1.y) - (p0.x - p1.x) * (test.y - p1.y)
}

pub fn aabb_get_top_plane(aabb: &AABB) -> (Plane, PlaneBoundaries) {    
    let mut pos = aabb.position;
    pos.z += aabb.height;
    let plane = Plane::new(pos, glm::vec4(0.0, 0.0, 1.0, 0.0));
    let aabb_boundaries = PlaneBoundaries {
        xmin: -aabb.width + aabb.position.x,
        xmax: aabb.width + aabb.position.x,
        ymin: -aabb.depth + aabb.position.y,
        ymax: aabb.depth + aabb.position.y
    };

    (plane, aabb_boundaries)
}

//The returned plane's reference point is the intersection point
pub fn segment_plane_tallest_collision(segment: &LineSegment, planes: &[Plane]) -> Option<Plane> {    
    let mut max_height = -f32::INFINITY;
    let mut collision = None;
    for plane in planes.iter() {
        if let Some(point) = segment_intersect_plane(plane, &segment) {
            if point.z > max_height {
                max_height = point.z;
                collision = Some(Plane::new(point, plane.normal));
            }
        }
    }
    collision
}