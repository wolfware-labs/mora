use mora_span::{Db, Project, SourceFile};

use crate::defs::ItemId;
use crate::items::{ItemIdx, ItemKind, Visibility, item_tree, signature_refs};
use crate::resolve::{
    ResolveErrorKind, Resolved, Scope, effective_visibility, enclosing_types, resolve_path,
};
use crate::ty::{Construct, Field, FnSignature, Param, Signature, TupleElem, Ty, Unsupported};
use crate::type_ref::{PathRef, SignatureRefKind, TypeRef};

/// The signature of an item, with the names in its types looked up (§14.5).
/// It reads no text range, so an edit inside a body does not run it again.
pub fn signature(db: &dyn Db, item: ItemId) -> &Signature {
    &item_signature(db, item.project, item.file, item.item).signature
}

/// What is wrong with the types of an item's signature.
pub(crate) fn signature_errors(db: &dyn Db, item: ItemId) -> &[SignatureError] {
    &item_signature(db, item.project, item.file, item.item).errors
}

/// A signature and the errors found while lowering it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LoweredSignature {
    signature: Signature,
    errors: Vec<SignatureError>,
}

/// A path in a signature that is wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SignatureError {
    /// The path, by its position among the paths of the item's signature.
    pub(crate) path: usize,
    /// The segment the error is about, counting from zero.
    pub(crate) segment: usize,
    /// The text of that segment, in NFC.
    pub(crate) name: String,
    /// What is wrong.
    pub(crate) kind: SignatureErrorKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SignatureErrorKind {
    /// The path does not resolve from the scope (`MR0127`, `MR0128`,
    /// `MR0129`).
    Resolve(Scope, ResolveErrorKind),
    /// The path names something that is not a type (`MR0127`).
    NotAType(Resolved),
    /// The path names a type less visible than the member (`MR0123`).
    LessVisible {
        /// The type.
        ty: ItemId,
        /// The field it is the type of, by its position, or `None` for the
        /// item itself.
        field: Option<usize>,
        /// The effective visibility of the member.
        member: Visibility,
    },
}

#[salsa::tracked(returns(ref))]
fn item_signature(
    db: &dyn Db,
    project: Project,
    file: SourceFile,
    item: ItemIdx,
) -> LoweredSignature {
    let tree = item_tree(db, file);
    let refs = signature_refs(db, file);
    let id = ItemId {
        project,
        file,
        item,
    };
    let mut type_params = Vec::new();
    for ty in enclosing_types(tree, Some(item)) {
        type_params.extend(refs[index(ty)].type_params.iter().cloned());
    }
    let mut lower = LowerTypes {
        db,
        scope: Scope {
            project,
            file,
            // A function or a value has no members, so looking up from
            // inside it is looking up from the type around it.
            within: Some(item),
        },
        type_params,
        field: None,
        member: effective_visibility(db, id),
        errors: Vec::new(),
    };
    let signature = match &refs[index(item)].kind {
        SignatureRefKind::Function {
            params,
            ret,
            is_async,
        } => Signature::Function(FnSignature {
            params: params
                .iter()
                .map(|param| Param {
                    name: param.name.clone(),
                    mode: param.mode,
                    is_params: param.is_params,
                    has_default: param.has_default,
                    ty: lower.ty(&param.ty),
                })
                .collect(),
            ret: ret.as_ref().map_or(Ty::UNIT, |ret| lower.ty(ret)),
            is_async: *is_async,
        }),
        SignatureRefKind::Struct(fields) => {
            let owner = lower.member;
            let fields = fields
                .iter()
                .enumerate()
                .map(|(position, field)| {
                    lower.field = Some(position);
                    lower.member = owner.min(field.visibility);
                    Field {
                        name: field.name.clone(),
                        visibility: field.visibility,
                        ty: lower.ty(&field.ty),
                    }
                })
                .collect();
            Signature::Struct(fields)
        }
        SignatureRefKind::Value(ty) => Signature::Value(lower.ty(ty)),
        SignatureRefKind::None => Signature::None,
    };
    LoweredSignature {
        signature,
        errors: lower.errors,
    }
}

fn index(idx: ItemIdx) -> usize {
    u32::from(idx.into_raw()) as usize
}

struct LowerTypes<'db> {
    db: &'db dyn Db,
    /// Where the names are looked up from.
    scope: Scope,
    /// The type parameters of the item and of the types around it.
    type_params: Vec<String>,
    /// The field whose type is being lowered, `None` for the item itself.
    field: Option<usize>,
    /// The effective visibility of the member whose types are being lowered.
    member: Visibility,
    /// The errors so far.
    errors: Vec<SignatureError>,
}

impl LowerTypes<'_> {
    fn ty(&mut self, ty: &TypeRef) -> Ty {
        match ty {
            TypeRef::Primitive(primitive) => Ty::Primitive(*primitive),
            TypeRef::String => Ty::String,
            TypeRef::Tuple(elems) => Ty::Tuple(
                elems
                    .iter()
                    .map(|(name, ty)| TupleElem {
                        name: name.clone(),
                        ty: self.ty(ty),
                    })
                    .collect(),
            ),
            TypeRef::Path(path) => self.path(path),
            TypeRef::Array(elem) => self.unsupported(Construct::Array, None, [&**elem]),
            TypeRef::Optional(elem) => self.unsupported(Construct::Optional, None, [&**elem]),
            TypeRef::Weak(elem) => self.unsupported(Construct::Weak, None, [&**elem]),
            TypeRef::Any(elem) => self.unsupported(Construct::Any, None, [&**elem]),
            TypeRef::Function(params, ret) => {
                self.unsupported(Construct::Function, None, params.iter().chain([&**ret]))
            }
            TypeRef::Splice => self.unsupported(Construct::Splice, None, []),
            TypeRef::Missing => Ty::Error,
        }
    }

    fn unsupported<'a>(
        &mut self,
        construct: Construct,
        item: Option<ItemId>,
        parts: impl IntoIterator<Item = &'a TypeRef>,
    ) -> Ty {
        Ty::Unsupported(Unsupported {
            construct,
            item,
            parts: parts.into_iter().map(|part| self.ty(part)).collect(),
        })
    }

    fn path(&mut self, path: &PathRef) -> Ty {
        if self.type_params.contains(&path.segments[0]) {
            return self.unsupported(Construct::TypeParam, None, &path.args);
        }
        let error = |segment: usize, kind| SignatureError {
            path: path.index,
            segment,
            name: path.segments[segment].clone(),
            kind,
        };
        let resolved = match resolve_path(self.db, self.scope, &path.segments) {
            Ok(resolved) => resolved,
            Err(failed) => {
                let kind = SignatureErrorKind::Resolve(self.scope, failed.kind);
                self.errors.push(error(failed.segment, kind));
                // The arguments are still checked.
                for arg in &path.args {
                    self.ty(arg);
                }
                return Ty::Error;
            }
        };
        let last = path.segments.len() - 1;
        let item = match resolved {
            Resolved::Item(item) => item,
            Resolved::External => return self.unsupported(Construct::External, None, &path.args),
            other => {
                self.errors
                    .push(error(last, SignatureErrorKind::NotAType(other)));
                return Ty::Error;
            }
        };
        let construct = match crate::resolve::item_data(self.db, item).kind {
            ItemKind::Struct => Construct::Generic,
            ItemKind::Class => Construct::Class,
            ItemKind::Union => Construct::Union,
            ItemKind::Enum => Construct::Enum,
            ItemKind::Interface => Construct::Interface,
            ItemKind::Function | ItemKind::Macro | ItemKind::Field => {
                self.errors.push(error(
                    last,
                    SignatureErrorKind::NotAType(Resolved::Item(item)),
                ));
                return Ty::Error;
            }
        };
        if effective_visibility(self.db, item) < self.member {
            self.errors.push(error(
                last,
                SignatureErrorKind::LessVisible {
                    ty: item,
                    field: self.field,
                    member: self.member,
                },
            ));
        }
        let generic = path.generic
            || !signature_refs(self.db, item.file)[index(item.item)]
                .type_params
                .is_empty();
        if construct == Construct::Generic && !generic {
            return Ty::Struct(item);
        }
        self.unsupported(construct, Some(item), &path.args)
    }
}
