use crate::module::{Module, ModuleVisitor, ParamId};

use burn_tensor::{backend::ADBackend, Tensor};

use super::GradientsParams;

/// Accumulate gradients into a single [Gradients](ADBackend::Gradients) object.
pub struct GradientsAccumulator {
    grads: GradientsParams,
}

impl Default for GradientsAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

impl GradientsAccumulator {
    /// Create a new gradients accumulator.
    pub fn new() -> Self {
        Self {
            grads: GradientsParams::new(),
        }
    }
}

impl GradientsAccumulator {
    /// Accumulate the given gradients for each parameter in the given module.
    pub fn accumulate<B: ADBackend, M>(&mut self, module: &M, grads: GradientsParams)
    where
        M: Module<Backend = B>,
    {
        let mut visitor = ModuleGradsAccumulator::new(&mut self.grads, grads);
        module.visit(&mut visitor);
    }

    /// Return the accumulated gradients and reset the accumulator state.
    pub fn grads(&mut self) -> GradientsParams {
        let mut grads = GradientsParams::new();
        core::mem::swap(&mut self.grads, &mut grads);

        grads
    }
}

#[derive(new)]
struct ModuleGradsAccumulator<'a> {
    grads: &'a mut GradientsParams,
    grads_new: GradientsParams,
}

impl<'a, B: ADBackend> ModuleVisitor<B> for ModuleGradsAccumulator<'a> {
    fn visit<const D: usize>(&mut self, id: &ParamId, _tensor: &Tensor<B, D>) {
        let grad_updated = match self.grads_new.remove::<B::InnerBackend, D>(id) {
            Some(new) => match self.grads.remove::<B::InnerBackend, D>(id) {
                Some(grad) => grad.add(new),
                None => new,
            },
            None => match self.grads.remove::<B::InnerBackend, D>(id) {
                Some(grad) => grad,
                None => return,
            },
        };

        self.grads
            .register::<B::InnerBackend, D>(id.clone(), grad_updated);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        nn::{Linear, LinearConfig},
        TestADBackend,
    };
    use burn_tensor::Distribution;

    #[test]
    fn test_accumulate_gradients_one_step() {
        let mut accumulator = GradientsAccumulator::new();
        let layer = layer();
        let loss = layer.forward(random_tensor());
        let grads = GradientsParams::from_grads(loss.backward(), &layer);

        accumulator.accumulate(&layer, grads);

        let grads = accumulator.grads();
        assert!(!grads.is_empty())
    }

    #[test]
    fn test_accumulate_gradients_two_steps() {
        let mut accumulator = GradientsAccumulator::new();
        let layer = layer();
        let loss_1 = layer.forward(random_tensor());
        let loss_2 = layer.forward(random_tensor());
        let grads_1 = GradientsParams::from_grads(loss_1.backward(), &layer);
        let grads_2 = GradientsParams::from_grads(loss_2.backward(), &layer);

        accumulator.accumulate(&layer, grads_1);
        accumulator.accumulate(&layer, grads_2);

        let grads = accumulator.grads();
        assert_eq!(grads.len(), 2)
    }

    fn layer() -> Linear<TestADBackend> {
        Linear::<TestADBackend>::new(&LinearConfig::new(20, 20).with_bias(true))
    }

    fn random_tensor() -> Tensor<TestADBackend, 2> {
        Tensor::<TestADBackend, 2>::random([2, 20], Distribution::Standard)
    }
}