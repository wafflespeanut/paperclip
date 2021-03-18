use super::{
    models::{
        Either, HttpMethod, Reference, Resolvable, ResolvableParameter, ResolvablePathItem,
        ResolvableResponse,
    },
    Schema,
};
use crate::error::ValidationError;
use heck::CamelCase;

use std::{cell::RefCell, collections::BTreeMap, mem};

// FIXME: The resolver is not in its best. It "just" works atm.

const DEF_REF_PREFIX: &str = "#/definitions/";
const PARAM_REF_PREFIX: &str = "#/parameters/";
const RESP_REF_PREFIX: &str = "#/responses/";

type DefinitionsMap<S> = BTreeMap<String, Resolvable<S>>;
type OperationsMap<S> = BTreeMap<String, ResolvablePathItem<S>>;
type ParametersMap<S> = BTreeMap<String, ResolvableParameter<S>>;
type ResponsesMap<S> = BTreeMap<String, ResolvableResponse<S>>;

/// API schema resolver. This visits each definition and resolves
/// `$ref` field (if any) by finding the associated definition and
/// replacing the field with a reference to the actual definition.
// FIXME: Move all validation to resolver.
pub(crate) struct Resolver<S> {
    /// List of definitions that must be marked as cyclic while resolving a definition.
    cyclic_defs: RefCell<Vec<Resolvable<S>>>,
    /// Globally defined object definitions.
    pub defs: DefinitionsMap<S>,
    /// Paths and the corresponding operations.
    pub paths: OperationsMap<S>,
    /// Globally defined parameters.
    pub params: ParametersMap<S>,
    /// Globally defined responses;
    pub resp: ResponsesMap<S>,
}

impl<S>
    From<(
        DefinitionsMap<S>,
        OperationsMap<S>,
        ParametersMap<S>,
        ResponsesMap<S>,
    )> for Resolver<S>
{
    fn from(
        (defs, paths, params, resp): (
            DefinitionsMap<S>,
            OperationsMap<S>,
            ParametersMap<S>,
            ResponsesMap<S>,
        ),
    ) -> Self {
        Resolver {
            cyclic_defs: vec![].into(),
            defs,
            paths,
            params,
            resp,
        }
    }
}

impl<S> Resolver<S>
where
    S: Schema + Default,
{
    /// Visit definitions and resolve them!
    pub fn resolve(&mut self) -> Result<(), ValidationError> {
        // Resolve path operations first. We may encounter anonymous
        // definitions along the way, which we'll insert into `self.defs`
        // and we'll have to resolve them anyway.
        let mut paths = mem::replace(&mut self.paths, BTreeMap::new());
        paths.iter_mut().try_for_each(|(path, map)| {
            log::trace!("Checking path: {}", path);
            self.resolve_operations(path, map)
        })?;
        self.paths = paths;

        // Set the names of all schemas.
        for (name, schema) in &self.defs {
            schema.write().set_name(name);
        }

        for (name, schema) in &self.defs {
            log::trace!("Entering: {}", name);
            self.resolve_definitions_no_root_ref(schema)?;

            for def in self.cyclic_defs.borrow_mut().drain(..) {
                log::debug!(
                    "Cyclic definition detected: {:?}",
                    def.read().name().unwrap()
                );
                def.write().set_cyclic(true);
            }
        }

        Ok(())
    }

    /// We've passed some definition. Resolve it assuming that it doesn't
    /// contain any reference.
    fn resolve_definitions_no_root_ref(
        &self,
        schema: &Resolvable<S>,
    ) -> Result<(), ValidationError> {
        let mut schema = match schema.try_write() {
            Some(s) => s,
            None => {
                self.cyclic_defs.borrow_mut().push(schema.clone());
                return Ok(());
            }
        };

        if let Some(inner) = schema.items_mut().take() {
            return self.resolve_definitions(inner);
        }

        if let Some(props) = schema.properties_mut().take() {
            props.iter_mut().try_for_each(|(k, s)| {
                log::trace!("Resolving property {:?}", k);
                self.resolve_definitions(s)
            })?;
        }

        if let Some(props) = schema
            .additional_properties_mut()
            .take()
            .and_then(|s| s.right_mut())
        {
            self.resolve_definitions(props)?;
        }

        Ok(())
    }

    /// Resolve the given definition. If it contains a reference, find and assign it,
    /// otherwise traverse further.
    fn resolve_definitions(&self, schema: &mut Resolvable<S>) -> Result<(), ValidationError> {
        let ref_def = {
            let s = match schema.try_read() {
                Some(s) => s,
                None => {
                    self.cyclic_defs.borrow_mut().push(schema.clone());
                    return Ok(());
                }
            };

            if let Some(ref_name) = s.reference() {
                log::trace!("Resolving definition {}", ref_name);
                Some(self.resolve_definition_reference(ref_name)?)
            } else {
                None
            }
        };

        if let Some(new) = ref_def {
            *schema = match schema {
                Resolvable::Raw(old) => Resolvable::Resolved {
                    old: old.clone(),
                    new: (&*new).clone(),
                },
                _ => unimplemented!("schema already resolved?"),
            };
        }

        self.resolve_definitions_no_root_ref(&*schema)
    }

    /// Resolve a given operation.
    fn resolve_operations(
        &mut self,
        path: &str,
        map: &mut ResolvablePathItem<S>,
    ) -> Result<(), ValidationError> {
        for (&method, op) in &mut map.methods {
            self.resolve_parameters(Some(method), path, &mut op.parameters)?;
            for resp in op.responses.values_mut() {
                let ref_resp = if let Some(r) = resp.left() {
                    log::trace!("Resolving response {}", r.reference);
                    Some(self.resolve_response_reference(&r.reference)?)
                } else {
                    None
                };

                if let Some(new) = ref_resp {
                    *resp = Either::Right(new);
                }

                let mut response = resp.write();
                self.resolve_operation_schema(
                    &mut response.schema,
                    Some(method),
                    path,
                    "Response",
                )?;
            }
        }

        self.resolve_parameters(None, path, &mut map.parameters)
    }

    /// Resolve the given bunch of parameters.
    fn resolve_parameters(
        &mut self,
        method: Option<HttpMethod>,
        path: &str,
        params: &mut Vec<Either<Reference, ResolvableParameter<S>>>,
    ) -> Result<(), ValidationError> {
        for p in params.iter_mut() {
            let ref_param = if let Some(r) = p.left() {
                log::trace!("Resolving parameter {}", r.reference);
                Some(self.resolve_parameter_reference(&r.reference)?)
            } else {
                None
            };

            if let Some(new) = ref_param {
                *p = Either::Right(new);
            }

            let mut param = p.write();
            self.resolve_operation_schema(&mut param.schema, method, path, "Body")?;
        }

        Ok(())
    }

    /// Resolves request/response schema in operation.
    fn resolve_operation_schema(
        &mut self,
        s: &mut Option<Resolvable<S>>,
        method: Option<HttpMethod>,
        path: &str,
        suffix: &str,
    ) -> Result<(), ValidationError> {
        let schema = match s.as_mut() {
            Some(s) => s,
            _ => return Ok(()),
        };

        match schema {
            Resolvable::Raw(ref s) if s.read().reference().is_none() => {
                // We've encountered an anonymous schema definition in some
                // parameter/response. Give it a name and add it to global definitions.
                let prefix = method.map(|s| s.to_string()).unwrap_or_default();
                let def_name = (prefix + path + suffix).to_camel_case();
                let mut ref_schema = S::default();
                ref_schema.set_reference(format!("{}{}", DEF_REF_PREFIX, def_name));
                let old_schema = mem::replace(schema, ref_schema.into());
                self.defs.insert(def_name, old_schema);
            }
            _ => (),
        }

        self.resolve_definitions(schema)?;
        Ok(())
    }

    /// Given a name (from `$ref` field), get a reference to the definition.
    fn resolve_definition_reference(&self, name: &str) -> Result<Resolvable<S>, ValidationError> {
        if !name.starts_with(DEF_REF_PREFIX) {
            return Err(ValidationError::InvalidRefUri(name.into()));
        }

        let name = &name[DEF_REF_PREFIX.len()..];
        let schema = self
            .defs
            .get(name)
            .ok_or_else(|| ValidationError::MissingReference(name.into()))?;
        Ok(schema.clone())
    }

    /// Given a name (from `$ref` field), get a reference to the parameter.
    fn resolve_parameter_reference(
        &self,
        name: &str,
    ) -> Result<ResolvableParameter<S>, ValidationError> {
        if !name.starts_with(PARAM_REF_PREFIX) {
            return Err(ValidationError::InvalidRefUri(name.into()));
        }

        let name = &name[PARAM_REF_PREFIX.len()..];
        let param = self
            .params
            .get(name)
            .ok_or_else(|| ValidationError::MissingReference(name.into()))?;
        Ok(param.clone())
    }

    /// Given a name (from `$ref` field), get a reference to the response.
    fn resolve_response_reference(
        &self,
        name: &str,
    ) -> Result<ResolvableResponse<S>, ValidationError> {
        if !name.starts_with(RESP_REF_PREFIX) {
            return Err(ValidationError::InvalidRefUri(name.into()));
        }

        let name = &name[RESP_REF_PREFIX.len()..];
        let resp = self
            .resp
            .get(name)
            .ok_or_else(|| ValidationError::MissingReference(name.into()))?;
        Ok(resp.clone())
    }
}
