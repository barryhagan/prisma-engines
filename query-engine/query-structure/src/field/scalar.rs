use crate::{DefaultKind, NativeTypeInstance, ValueGenerator, ast, parent_container::ParentContainer, prelude::*};
use chrono::{DateTime, FixedOffset};
use psl::{
    generators::{DEFAULT_CUID_VERSION, DEFAULT_UUID_VERSION},
    parser_database::{self as db, ScalarFieldType, ScalarType, walkers},
    schema_ast::ast::FieldArity,
};
use std::fmt::{Debug, Display};

pub type ScalarField = crate::Zipper<ScalarFieldId>;
pub type ScalarFieldRef = ScalarField;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScalarFieldId {
    InModel(psl::parser_database::ScalarFieldId),
    InCompositeType((db::CompositeTypeId, ast::FieldId)),
}

impl ScalarField {
    pub fn is_id(&self) -> bool {
        match self.id {
            ScalarFieldId::InModel(id) => self.dm.walk(id).is_single_pk(),
            ScalarFieldId::InCompositeType(_) => false,
        }
    }

    pub fn is_list(&self) -> bool {
        matches!(self.arity(), FieldArity::List)
    }

    pub fn is_required(&self) -> bool {
        matches!(self.arity(), FieldArity::Required)
    }

    pub fn unique(&self) -> bool {
        match self.id {
            ScalarFieldId::InModel(id) => self.dm.walk(id).is_unique(),
            ScalarFieldId::InCompositeType(_) => false, // TODO: is this right?
        }
    }

    pub fn db_name(&self) -> &str {
        match self.id {
            ScalarFieldId::InModel(id) => self.dm.walk(id).database_name(),
            ScalarFieldId::InCompositeType(id) => self.dm.walk(id).database_name(),
        }
    }

    pub fn type_identifier_with_arity(&self) -> (TypeIdentifier, FieldArity) {
        (self.type_identifier(), self.arity())
    }

    pub fn is_read_only(&self) -> bool {
        let sfid = match self.id {
            ScalarFieldId::InModel(id) => id,
            ScalarFieldId::InCompositeType(_) => return false,
        };
        let sf = self.dm.walk(sfid);
        let mut relation_fields = sf.model().relation_fields();
        relation_fields.any(|rf| rf.fields().into_iter().flatten().any(|sf2| sf.id == sf2.id))
    }

    pub fn is_numeric(&self) -> bool {
        self.type_identifier().is_numeric()
    }

    pub fn container(&self) -> ParentContainer {
        match self.id {
            ScalarFieldId::InModel(id) => self.dm.find_model_by_id(self.dm.walk(id).model().id).into(),
            ScalarFieldId::InCompositeType((id, _)) => self.dm.find_composite_type_by_id(id).into(),
        }
    }

    pub fn borrowed_name<'a>(&self, schema: &'a psl::ValidatedSchema) -> &'a str {
        match self.id {
            ScalarFieldId::InModel(id) => schema.db.walk(id).name(),
            ScalarFieldId::InCompositeType(id) => schema.db.walk(id).name(),
        }
    }

    pub fn name(&self) -> &str {
        match self.id {
            ScalarFieldId::InModel(id) => self.dm.walk(id).name(),
            ScalarFieldId::InCompositeType(id) => self.dm.walk(id).name(),
        }
    }

    pub fn type_identifier(&self) -> TypeIdentifier {
        let scalar_field_type = match self.id {
            ScalarFieldId::InModel(id) => self.dm.walk(id).scalar_field_type(),
            ScalarFieldId::InCompositeType(id) => self.dm.walk(id).r#type(),
        };

        match scalar_field_type {
            ScalarFieldType::CompositeType(_) => {
                unreachable!("This shouldn't be reached; composite types are not supported in compound unique indices.",)
            }
            ScalarFieldType::Enum(x) => TypeIdentifier::Enum(x),
            ScalarFieldType::BuiltInScalar(scalar) => scalar.into(),
            ScalarFieldType::Unsupported(_) => TypeIdentifier::Unsupported,
        }
    }

    pub fn arity(&self) -> FieldArity {
        match self.id {
            ScalarFieldId::InModel(id) => self.dm.walk(id).ast_field().arity,
            ScalarFieldId::InCompositeType(id) => self.dm.walk(id).arity(),
        }
    }

    pub fn base_type(&self) -> Type {
        self.dm.clone().zip(self.type_identifier())
    }

    pub fn type_info(&self) -> FieldTypeInformation {
        FieldTypeInformation {
            typ: self.base_type(),
            arity: self.arity(),
            native_type: self.native_type(),
        }
    }

    pub fn internal_enum(&self) -> Option<crate::InternalEnum> {
        let enum_id = match self.id {
            ScalarFieldId::InModel(id) => self.dm.walk(id).scalar_field_type().as_enum(),
            ScalarFieldId::InCompositeType(id) => self.dm.walk(id).r#type().as_enum(),
        }?;
        Some(self.dm.clone().zip(enum_id))
    }

    pub fn default_value(&self) -> Option<DefaultKind> {
        match self.id {
            ScalarFieldId::InModel(id) => {
                let walker = self.dm.walk(id);
                walker
                    .default_value()
                    .map(|dv| dml_default_kind(&dv.ast_attribute().arguments.arguments[0].value, walker.scalar_type()))
            }
            ScalarFieldId::InCompositeType(id) => {
                let walker = self.dm.walk(id);
                walker
                    .default_value()
                    .map(|dv| dml_default_kind(dv, walker.scalar_type()))
            }
        }
    }

    pub fn is_updated_at(&self) -> bool {
        match self.id {
            ScalarFieldId::InModel(id) => self.dm.walk(id).is_updated_at(),
            ScalarFieldId::InCompositeType(_) => false,
        }
    }

    pub fn is_auto_generated_int_id(&self) -> bool {
        match self.id {
            ScalarFieldId::InModel(id) => {
                let walker = self.dm.walk(id);
                walker.is_single_pk()
                    && matches!(
                        walker.default_value().map(|v| v.value()),
                        Some(ast::Expression::Function(name, _, _)) if name == "autoincrement" || name == "sequence"
                    )
                    && matches!(walker.scalar_type(), Some(psl::parser_database::ScalarType::Int))
            }
            ScalarFieldId::InCompositeType(_) => false,
        }
    }

    pub fn native_type(&self) -> Option<NativeTypeInstance> {
        let connector = self.dm.schema.connector;

        let raw_nt = match self.id {
            ScalarFieldId::InModel(id) => self.dm.walk(id).raw_native_type(),
            ScalarFieldId::InCompositeType(id) => self.dm.walk(id).raw_native_type(),
        };

        let psl_nt = raw_nt
            .and_then(|(_, name, args, span)| connector.parse_native_type(name, args, span, &mut Default::default()));

        let scalar_type = match self.id {
            ScalarFieldId::InModel(id) => self.dm.walk(id).scalar_type(),
            ScalarFieldId::InCompositeType(id) => self.dm.walk(id).scalar_type(),
        };

        let nt = psl_nt.or_else(|| scalar_type.and_then(|st| connector.default_native_type_for_scalar_type(&st)))?;

        Some(NativeTypeInstance {
            native_type: nt,
            connector,
        })
    }

    pub fn can_be_compacted(&self) -> bool {
        let connector = self.dm.schema.connector;

        let nt = self.native_type().map(|nt| nt.native_type);

        connector.native_type_supports_compacting(nt)
    }

    pub fn parse_json_datetime(&self, value: &str) -> chrono::ParseResult<DateTime<FixedOffset>> {
        let nt = self.native_type().map(|nt| nt.native_type);
        let connector = self.dm.schema.connector;

        connector.parse_json_datetime(value, nt)
    }

    pub fn parse_json_bytes(&self, value: &str) -> PrismaValueResult<Vec<u8>> {
        let nt = self.native_type().map(|nt| nt.native_type);
        let connector = self.dm.schema.connector;

        connector.parse_json_bytes(value, nt)
    }

    pub fn is_autoincrement(&self) -> bool {
        match self.id {
            ScalarFieldId::InModel(id) => self.dm.walk(id).is_autoincrement(),
            ScalarFieldId::InCompositeType(_) => false,
        }
    }
}

impl Display for ScalarField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.container().name(), self.name())
    }
}

impl From<(InternalDataModelRef, walkers::IndexFieldWalker<'_>)> for ScalarFieldRef {
    fn from((dm, f): (InternalDataModelRef, walkers::IndexFieldWalker<'_>)) -> Self {
        match f {
            walkers::IndexFieldWalker::Scalar(sf) => dm.zip(ScalarFieldId::InModel(sf.id)),
            walkers::IndexFieldWalker::Composite(cf) => dm.zip(ScalarFieldId::InCompositeType(cf.id)),
        }
    }
}

pub fn dml_default_kind(default_value: &ast::Expression, scalar_type: Option<ScalarType>) -> DefaultKind {
    // This has all been validated in parser-database, so unwrapping is always safe.
    match default_value {
        ast::Expression::Function(funcname, args, _) if funcname == "dbgenerated" => {
            DefaultKind::Expression(ValueGenerator::new_dbgenerated(
                args.arguments
                    .first()
                    .and_then(|arg| arg.value.as_string_value())
                    .map(|(val, _)| val.to_owned())
                    .unwrap_or_else(String::new),
            ))
        }
        ast::Expression::Function(funcname, _, _) if funcname == "auto" => {
            DefaultKind::Expression(ValueGenerator::new_auto())
        }
        ast::Expression::Function(funcname, _args, _) if funcname == "autoincrement" => {
            DefaultKind::Expression(ValueGenerator::new_autoincrement())
        }
        ast::Expression::Function(funcname, _args, _) if funcname == "sequence" => {
            DefaultKind::Expression(ValueGenerator::new_sequence(Vec::new()))
        }
        ast::Expression::Function(funcname, args, _) if funcname == "uuid" => {
            let version = args
                .arguments
                .first()
                .and_then(|arg| arg.value.as_numeric_value())
                .map(|(val, _)| val.parse::<u8>().unwrap())
                .unwrap_or(DEFAULT_UUID_VERSION);

            DefaultKind::Expression(ValueGenerator::new_uuid(version))
        }
        ast::Expression::Function(funcname, args, _) if funcname == "cuid" => {
            let version = args
                .arguments
                .first()
                .and_then(|arg| arg.value.as_numeric_value())
                .map(|(val, _)| val.parse::<u8>().unwrap())
                .unwrap_or(DEFAULT_CUID_VERSION);
            DefaultKind::Expression(ValueGenerator::new_cuid(version))
        }
        ast::Expression::Function(funcname, _, _) if funcname == "ulid" => {
            DefaultKind::Expression(ValueGenerator::new_ulid())
        }
        ast::Expression::Function(funcname, args, _) if funcname == "nanoid" => {
            DefaultKind::Expression(ValueGenerator::new_nanoid(
                args.arguments
                    .first()
                    .and_then(|arg| arg.value.as_numeric_value())
                    .map(|(val, _)| val.parse::<u8>().unwrap()),
            ))
        }
        ast::Expression::Function(funcname, _args, _) if funcname == "now" => {
            DefaultKind::Expression(ValueGenerator::new_now())
        }
        ast::Expression::NumericValue(num, _) => match scalar_type {
            Some(ScalarType::Int) => DefaultKind::Single(PrismaValue::Int(num.parse().unwrap())),
            Some(ScalarType::BigInt) => DefaultKind::Single(PrismaValue::BigInt(num.parse().unwrap())),
            Some(ScalarType::Float) => DefaultKind::Single(PrismaValue::Float(num.parse().unwrap())),
            Some(ScalarType::Decimal) => DefaultKind::Single(PrismaValue::Float(num.parse().unwrap())),
            other => unreachable!("{:?}", other),
        },
        ast::Expression::ConstantValue(v, _) => match scalar_type {
            Some(ScalarType::Boolean) => DefaultKind::Single(PrismaValue::Boolean(v.parse().unwrap())),
            None => DefaultKind::Single(PrismaValue::Enum(v.to_owned())),
            other => unreachable!("{:?}", other),
        },
        ast::Expression::StringValue(v, _) => match scalar_type {
            Some(ScalarType::DateTime) => DefaultKind::Single(PrismaValue::DateTime(v.parse().unwrap())),
            Some(ScalarType::String) => DefaultKind::Single(PrismaValue::String(v.parse().unwrap())),
            Some(ScalarType::Json) => DefaultKind::Single(PrismaValue::Json(v.parse().unwrap())),
            Some(ScalarType::Decimal) => DefaultKind::Single(PrismaValue::Float(v.parse().unwrap())),
            Some(ScalarType::Bytes) => DefaultKind::Single(PrismaValue::Bytes(prisma_value::decode_bytes(v).unwrap())),
            other => unreachable!("{:?}", other),
        },
        ast::Expression::Array(values, _) => {
            let values = values
                .iter()
                .map(|expr| dml_default_kind(expr, scalar_type).unwrap_single())
                .collect();

            DefaultKind::Single(PrismaValue::List(values))
        }
        other => unreachable!("{:?}", other),
    }
}

impl std::fmt::Debug for ScalarField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ScalarField")
            .field(&format!("{}.{}", self.container().name(), self.name()))
            .finish()
    }
}
