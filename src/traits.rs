use ozy::collision::Sphere;

//Trait implemented by game objects that are represented by spheres
pub trait SphereCollider {
    fn sphere(&self) -> Sphere;
}

//Structs that have a 3D position
pub trait PositionAble {
    fn position(&self) -> glm::TVec3<f32>;
    fn set_position(&self, new_position: glm::TVec3<f32>);
}