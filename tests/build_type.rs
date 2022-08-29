#![cfg(not(feature = "no_object"))]
use rhai::{CustomType, Engine, EvalAltResult, Position, TypeBuilder, INT};

#[test]
fn build_type() -> Result<(), Box<EvalAltResult>> {
    #[derive(Debug, Clone, PartialEq)]
    struct Vec3 {
        x: INT,
        y: INT,
        z: INT,
    }

    impl Vec3 {
        fn new(x: INT, y: INT, z: INT) -> Self {
            Self { x, y, z }
        }
        fn get_x(&mut self) -> INT {
            self.x
        }
        fn set_x(&mut self, x: INT) {
            self.x = x
        }
        fn get_y(&mut self) -> INT {
            self.y
        }
        fn set_y(&mut self, y: INT) {
            self.y = y
        }
        fn get_z(&mut self) -> INT {
            self.z
        }
        fn set_z(&mut self, z: INT) {
            self.z = z
        }
        fn get_component(&mut self, idx: INT) -> Result<INT, Box<EvalAltResult>> {
            match idx {
                0 => Ok(self.x),
                1 => Ok(self.y),
                2 => Ok(self.z),
                _ => Err(Box::new(EvalAltResult::ErrorIndexNotFound(
                    idx.into(),
                    Position::NONE,
                ))),
            }
        }
    }

    impl IntoIterator for Vec3 {
        type Item = INT;
        type IntoIter = std::vec::IntoIter<Self::Item>;

        fn into_iter(self) -> Self::IntoIter {
            vec![self.x, self.y, self.z].into_iter()
        }
    }

    impl CustomType for Vec3 {
        fn build(mut builder: TypeBuilder<Self>) {
            builder
                .with_name("Vec3")
                .is_iterable()
                .with_fn("vec3", Self::new)
                .is_iterable()
                .with_get_set("x", Self::get_x, Self::set_x)
                .with_get_set("y", Self::get_y, Self::set_y)
                .with_get_set("z", Self::get_z, Self::set_z);

            #[cfg(not(feature = "no_index"))]
            builder.with_indexer_get(Self::get_component);
        }
    }

    let mut engine = Engine::new();
    engine.build_type::<Vec3>();

    assert_eq!(
        engine.eval::<Vec3>(
            "
                let v = vec3(1, 2, 3);
                v
            ",
        )?,
        Vec3::new(1, 2, 3),
    );
    assert_eq!(
        engine.eval::<INT>(
            "
                let v = vec3(1, 2, 3);
                v.x
            ",
        )?,
        1,
    );
    assert_eq!(
        engine.eval::<INT>(
            "
                let v = vec3(1, 2, 3);
                v.y
            ",
        )?,
        2,
    );
    assert_eq!(
        engine.eval::<INT>(
            "
                let v = vec3(1, 2, 3);
                v.z
            ",
        )?,
        3,
    );
    #[cfg(not(feature = "no_index"))]
    assert!(engine.eval::<bool>(
        "
            let v = vec3(1, 2, 3);
            v.x == v[0] && v.y == v[1] && v.z == v[2]
        ",
    )?);
    assert_eq!(
        engine.eval::<Vec3>(
            "
                let v = vec3(1, 2, 3);
                v.x = 5;
                v.y = 6;
                v.z = 7;
                v
            ",
        )?,
        Vec3::new(5, 6, 7),
    );
    assert_eq!(
        engine.eval::<INT>(
            "
                let sum = 0;
                let v = vec3(1, 2, 3);
                for i in v {
                    sum += i;
                }
                sum
            ",
        )?,
        6,
    );

    Ok(())
}
