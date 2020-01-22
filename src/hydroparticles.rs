use super::units::*;
use ggez::graphics::Rect;
use ggez::nalgebra as na;

use crate::smoothing_kernel;
use crate::smoothing_kernel::Kernel;

pub struct HydroParticles {
    pub positions: Vec<Point>,
    pub velocities: Vec<Vector>,
    pub accellerations: Vec<Vector>,

    pub boundary_particles: Vec<Point>, // also called "shadow particles", immovable particles used for boundaries

    smoothing_length_sq: Real, // typically expressed as 'h'
    particle_density: Real,    // #particles/m² for resting fluid
    fluid_density: Real,       // kg/m² for the resting fluid (ρ, rho)
    fluid_speedofsound_sq: Real, // speed of sound in this fluid squared
    fluid_viscosity: Real,     // the dynamic viscosity of this fluid in Pa*s (μ, mu)

    density_kernel: smoothing_kernel::Poly6,
    pressure_kernel: smoothing_kernel::Spiky,
    viscosity_kernel: smoothing_kernel::Viscosity,

    boundary_force_factor: Real,

    densities: Vec<Real>, // Local densities ρ
}

impl HydroParticles {
    pub fn new(
        smoothing_factor: Real,
        particle_density: Real, // #particles/m² for resting fluid
        fluid_density: Real,    // kg/m² for the resting fluid
        fluid_speedofsound: Real, // speed of sound in this fluid
        fluid_viscosity: Real,  // the dynamic viscosity of this fluid in Pa*s (μ, mu)
    ) -> HydroParticles {
        let smoothing_length = 2.0 * Self::particle_radius_from_particle_density(particle_density) * smoothing_factor;
        HydroParticles {
            positions: Vec::new(),
            velocities: Vec::new(),
            accellerations: Vec::new(),

            boundary_particles: Vec::new(),

            smoothing_length_sq: smoothing_length * smoothing_length,
            particle_density: particle_density,
            fluid_density: fluid_density,
            fluid_speedofsound_sq: fluid_speedofsound * fluid_speedofsound,
            fluid_viscosity: fluid_viscosity,

            density_kernel: smoothing_kernel::Poly6::new(smoothing_length),
            pressure_kernel: smoothing_kernel::Spiky::new(smoothing_length),
            viscosity_kernel: smoothing_kernel::Viscosity::new(smoothing_length),

            boundary_force_factor: 10.0, // (expected accelleration) / (spacing ratio of boundary / normal particles)

            densities: Vec::new(),
        }
    }

    pub fn particle_mass(&self) -> Real {
        self.fluid_density / self.particle_density
    }

    fn pressure(&self, local_density: Real) -> Real {
        // Isothermal gas (== Tait equation for water-like fluids with gamma 1)
        self.fluid_speedofsound_sq * (local_density - self.fluid_density)
    }

    fn particle_radius_from_particle_density(particle_density: Real) -> Real {
        // density is per m²
        0.5 / particle_density.sqrt()
    }

    fn num_particles_per_meter(&self) -> Real {
        self.particle_density.sqrt()
    }

    pub fn suggested_particle_render_radius(&self) -> Real {
        Self::particle_radius_from_particle_density(self.particle_density)
    }

    /// - `jitter`: Amount of jitter. 0 for perfect lattice. >1 and particles are no longer in a strict lattice.
    pub fn add_fluid_rect(&mut self, fluid_rect: &Rect, jitter_amount: Real) {
        // fluid_rect.w * fluid_rect.h / self.particle_density, but discretized per axis
        let num_particles_per_meter = self.num_particles_per_meter();
        let num_particles_x = std::cmp::max(1, (fluid_rect.w as Real * num_particles_per_meter) as usize);
        let num_particles_y = std::cmp::max(1, (fluid_rect.h as Real * num_particles_per_meter) as usize);
        let num_particles = num_particles_x * num_particles_y;

        self.positions.reserve(num_particles);
        self.velocities.resize(self.velocities.len() + num_particles, na::zero());
        self.densities.resize(self.densities.len() + num_particles, na::zero());
        self.accellerations.resize(self.accellerations.len() + num_particles, na::zero());

        let bottom_left = Point::new(fluid_rect.x as Real, fluid_rect.y as Real);
        let step = (fluid_rect.w as Real / (num_particles_x as Real)).min(fluid_rect.h as Real / (num_particles_y as Real));
        let jitter_factor = step * jitter_amount;
        for y in 0..num_particles_y {
            for x in 0..num_particles_x {
                let jitter = (Vector::new_random() * 0.5 + Vector::new(0.5, 0.5)) * jitter_factor;
                self.positions
                    .push(bottom_left + jitter + na::Vector2::new(step * (x as Real), step * (y as Real)));
            }
        }
    }

    pub fn add_boundary_line(&mut self, start: &Point, end: &Point) {
        let distance = na::distance(start, end);
        let num_particles_per_meter = self.num_particles_per_meter();
        let num_shadow_particles = std::cmp::max(1, (distance * num_particles_per_meter) as usize);
        self.boundary_particles.reserve(num_shadow_particles);
        let step = (end - start) / (num_shadow_particles as Real);

        let mut pos = *start;
        for _ in 0..num_shadow_particles {
            self.boundary_particles.push(pos);
            pos += step;
        }
    }

    fn update_densities(&mut self) {
        assert_eq!(self.positions.len(), self.densities.len());

        for density in self.densities.iter_mut() {
            *density = 0.0;
        }

        let mass = self.particle_mass();

        // Density contributions are symmetric!
        for (i, ri) in self.positions.iter().enumerate() {
            self.densities[i] += self.density_kernel.evaluate(0.0, 0.0) * mass; // self-contribution
            for (j, rj) in self.positions.iter().enumerate().skip(i + 1) {
                let r_sq = na::distance_squared(ri, rj);
                if r_sq > self.smoothing_length_sq {
                    continue;
                }
                let density_contribution = self.density_kernel.evaluate(r_sq, r_sq.sqrt()) * mass;
                self.densities[i] += density_contribution;
                self.densities[j] += density_contribution;
            }
            for rj in self.boundary_particles.iter() {
                let r_sq = na::distance_squared(ri, rj);
                if r_sq > self.smoothing_length_sq {
                    continue;
                }
                let density_contribution = self.density_kernel.evaluate(r_sq, r_sq.sqrt()) * mass;
                self.densities[i] += density_contribution;
            }
        }
    }

    fn update_accellerations(&mut self) {
        let mass = self.particle_mass();

        let gravity = Vector::new(0.0, -9.81) * 0.01;
        for a in self.accellerations.iter_mut() {
            *a = gravity;
        }

        // pressure & viscosity forces
        // It's done in a symmetric way
        // According to https://www8.cs.umu.se/kurser/TDBD24/VT06/lectures/sphsurvivalkit.pdf
        // the "good way" to do symmetric forces in SPH is -m (pi + pj) / (2 * rhoj * rhoi)
        for (i, (ri, rhoi)) in self.positions.iter().zip(self.densities.iter()).enumerate() {
            let pi = self.pressure(*rhoi);

            // no self-contribution since vector to particle is zero (-> no pressure) and velocity difference is zero as well (-> no viscosity)
            for (j, (rj, rhoj)) in self.positions.iter().zip(self.densities.iter()).enumerate().skip(i + 1) {
                let ri_rj = ri - rj;
                let r_sq = ri_rj.norm_squared();
                if r_sq > self.smoothing_length_sq {
                    continue;
                }
                let r = r_sq.sqrt();

                let pj = self.pressure(*rhoj);

                // accelleration from pressure force
                // As in "Particle-Based Fluid Simulation for Interactive Applications", Müller et al.
                let fpressure_unsmoothed = -mass * (pi + pj) / (2.0 * rhoi * rhoj);
                assert!(!fpressure_unsmoothed.is_nan());
                assert!(!fpressure_unsmoothed.is_infinite());
                let fpressure = fpressure_unsmoothed * self.pressure_kernel.gradient(ri_rj, r_sq, r);

                // accelleration from viscosity force
                // As in "Particle-Based Fluid Simulation for Interactive Applications", Müller et al.
                let velocitydiff = self.velocities[j] - self.velocities[i];
                let fviscosity = self.fluid_viscosity * mass * self.viscosity_kernel.laplacian(r_sq, r) / (rhoi * rhoj) * velocitydiff;

                let ftotal = fpressure + fviscosity;

                // Symmetric!
                self.accellerations[i] += ftotal;
                self.accellerations[j] -= ftotal;
            }

            // Boundary forces as described by
            // "SPH particle boundary forces for arbitrary boundaries" by Monaghan and Kajtar 2009
            // Simple formulation found in http://www.unige.ch/math/folks/sutti/SPH_2019.pdf under 2.3.4 Radial force
            // ("SPH treatment of boundaries and application to moving objects" by Marco Sutti)
            for rj in self.boundary_particles.iter() {
                let ri_rj = ri - rj;
                let r_sq = ri_rj.norm_squared();
                if r_sq > self.smoothing_length_sq {
                    continue;
                }
                self.accellerations[i] += self.boundary_force_factor * self.density_kernel.evaluate(r_sq, r_sq.sqrt()) / r_sq * ri_rj;
            }
        }
    }

    pub fn physics_step(&mut self, dt: Real) {
        assert_eq!(self.positions.len(), self.velocities.len());
        assert_eq!(self.positions.len(), self.accellerations.len());

        // leap frog integration scheme with integer steps
        // https://en.wikipedia.org/wiki/Leapfrog_integration
        for ((pos, v), a) in self.positions.iter_mut().zip(self.velocities.iter_mut()).zip(self.accellerations.iter()) {
            *pos += *v * dt + a * (0.5 * dt * dt);
            // partial update of velocity.
            // what we want is v_new = v_old + 0.5 (a_old + a_new) () t
            // spit it to: v_almostnew = v_old + 0.5 * a_old * t + 0.5 * a_new * t
            *v += 0.5 * dt * a;
        }

        self.update_densities();
        self.update_accellerations();

        // part 2 of leap frog integration. Finish updating velocity.
        for (v, a) in self.velocities.iter_mut().zip(self.accellerations.iter()) {
            *v += 0.5 * dt * a;
        }
    }
}
