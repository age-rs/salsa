use std::any::{Any, TypeId};
use std::fmt;
use std::ops::IndexMut;

pub mod input_field;
pub mod setter;
pub mod singleton;

use input_field::FieldIngredientImpl;

use crate::cycle::CycleHeads;
use crate::function::VerifyResult;
use crate::id::{AsId, FromId, FromIdWithDb};
use crate::ingredient::Ingredient;
use crate::input::singleton::{Singleton, SingletonChoice};
use crate::key::DatabaseKeyIndex;
use crate::plumbing::Jar;
use crate::sync::Arc;
use crate::table::memo::{MemoTable, MemoTableTypes};
use crate::table::{Slot, Table};
use crate::zalsa::{IngredientIndex, Zalsa};
use crate::{zalsa_local, Durability, Id, Revision, Runtime};

pub trait Configuration: Any {
    const DEBUG_NAME: &'static str;
    const FIELD_DEBUG_NAMES: &'static [&'static str];
    const LOCATION: crate::ingredient::Location;

    /// The singleton state for this input if any.
    type Singleton: SingletonChoice + Send + Sync;

    /// The input struct (which wraps an `Id`)
    type Struct: FromId + AsId + 'static + Send + Sync;

    /// A (possibly empty) tuple of the fields for this struct.
    type Fields: Send + Sync;

    /// A array of [`Revision`], one per each of the value fields.
    type Revisions: Send + Sync + fmt::Debug + IndexMut<usize, Output = Revision>;

    /// A array of [`Durability`], one per each of the value fields.
    type Durabilities: Send + Sync + fmt::Debug + IndexMut<usize, Output = Durability>;
}

pub struct JarImpl<C: Configuration> {
    _phantom: std::marker::PhantomData<C>,
}

impl<C: Configuration> Default for JarImpl<C> {
    fn default() -> Self {
        Self {
            _phantom: Default::default(),
        }
    }
}

impl<C: Configuration> Jar for JarImpl<C> {
    fn create_ingredients(
        _zalsa: &mut Zalsa,
        struct_index: crate::zalsa::IngredientIndex,
    ) -> Vec<Box<dyn Ingredient>> {
        let struct_ingredient: IngredientImpl<C> = IngredientImpl::new(struct_index);

        std::iter::once(Box::new(struct_ingredient) as _)
            .chain((0..C::FIELD_DEBUG_NAMES.len()).map(|field_index| {
                Box::new(<FieldIngredientImpl<C>>::new(struct_index, field_index)) as _
            }))
            .collect()
    }

    fn id_struct_type_id() -> TypeId {
        TypeId::of::<C::Struct>()
    }
}

pub struct IngredientImpl<C: Configuration> {
    ingredient_index: IngredientIndex,
    singleton: C::Singleton,
    memo_table_types: Arc<MemoTableTypes>,
    _phantom: std::marker::PhantomData<C::Struct>,
}

impl<C: Configuration> IngredientImpl<C> {
    pub fn new(index: IngredientIndex) -> Self {
        Self {
            ingredient_index: index,
            singleton: Default::default(),
            memo_table_types: Arc::new(MemoTableTypes::default()),
            _phantom: std::marker::PhantomData,
        }
    }

    fn data(zalsa: &Zalsa, id: Id) -> &Value<C> {
        zalsa.table().get(id)
    }

    fn data_raw(table: &Table, id: Id) -> *mut Value<C> {
        table.get_raw(id)
    }

    pub fn database_key_index(&self, id: C::Struct) -> DatabaseKeyIndex {
        DatabaseKeyIndex::new(self.ingredient_index, id.as_id())
    }

    pub fn new_input(
        &self,
        zalsa: &Zalsa,
        zalsa_local: &zalsa_local::ZalsaLocal,
        fields: C::Fields,
        revisions: C::Revisions,
        durabilities: C::Durabilities,
    ) -> C::Struct {
        let id = self.singleton.with_scope(|| {
            let (id, _) = zalsa_local.allocate(zalsa, self.ingredient_index, |_| Value::<C> {
                fields,
                revisions,
                durabilities,
                // SAFETY: We only ever access the memos of a value that we allocated through
                // our `MemoTableTypes`.
                memos: unsafe { MemoTable::new(self.memo_table_types()) },
            });

            id
        });

        FromIdWithDb::from_id(id, zalsa)
    }

    /// Change the value of the field `field_index` to a new value.
    ///
    /// # Parameters
    ///
    /// * `runtime`, the salsa runtiem
    /// * `id`, id of the input struct
    /// * `field_index`, index of the field that will be changed
    /// * `durability`, durability of the new value. If omitted, uses the durability of the previous value.
    /// * `setter`, function that modifies the fields tuple; should only modify the element for `field_index`
    pub fn set_field<R>(
        &mut self,
        runtime: &mut Runtime,
        id: C::Struct,
        field_index: usize,
        durability: Option<Durability>,
        setter: impl FnOnce(&mut C::Fields) -> R,
    ) -> R {
        let id: Id = id.as_id();

        let data_raw = Self::data_raw(runtime.table(), id);

        // SAFETY: We hold `&mut` on the runtime so no `&`-references can be active.
        // Also, we don't access any other data from the table while `r` is active.
        let data = unsafe { &mut *data_raw };

        data.revisions[field_index] = runtime.current_revision();

        let field_durability = &mut data.durabilities[field_index];
        if *field_durability != Durability::MIN {
            runtime.report_tracked_write(*field_durability);
        }
        *field_durability = durability.unwrap_or(*field_durability);

        setter(&mut data.fields)
    }

    /// Get the singleton input previously created (if any).
    #[doc(hidden)]
    pub fn get_singleton_input(&self, zalsa: &Zalsa) -> Option<C::Struct>
    where
        C: Configuration<Singleton = Singleton>,
    {
        self.singleton
            .index()
            .map(|id| FromIdWithDb::from_id(id, zalsa))
    }

    /// Access field of an input.
    /// Note that this function returns the entire tuple of value fields.
    /// The caller is responsible for selecting the appropriate element.
    pub fn field<'db>(
        &'db self,
        zalsa: &'db Zalsa,
        zalsa_local: &'db zalsa_local::ZalsaLocal,
        id: C::Struct,
        field_index: usize,
    ) -> &'db C::Fields {
        let field_ingredient_index = self.ingredient_index.successor(field_index);
        let id = id.as_id();
        let value = Self::data(zalsa, id);
        let durability = value.durabilities[field_index];
        let revision = value.revisions[field_index];
        zalsa_local.report_tracked_read_simple(
            DatabaseKeyIndex::new(field_ingredient_index, id),
            durability,
            revision,
        );
        &value.fields
    }

    #[cfg(feature = "salsa_unstable")]
    /// Returns all data corresponding to the input struct.
    pub fn entries<'db>(&'db self, zalsa: &'db Zalsa) -> impl Iterator<Item = &'db Value<C>> {
        zalsa.table().slots_of::<Value<C>>()
    }

    /// Peek at the field values without recording any read dependency.
    /// Used for debug printouts.
    pub fn leak_fields<'db>(&'db self, zalsa: &'db Zalsa, id: C::Struct) -> &'db C::Fields {
        let id = id.as_id();
        let value = Self::data(zalsa, id);
        &value.fields
    }
}

impl<C: Configuration> Ingredient for IngredientImpl<C> {
    fn location(&self) -> &'static crate::ingredient::Location {
        &C::LOCATION
    }

    fn ingredient_index(&self) -> IngredientIndex {
        self.ingredient_index
    }

    unsafe fn maybe_changed_after(
        &self,
        _zalsa: &crate::zalsa::Zalsa,
        _db: crate::database::RawDatabase<'_>,
        _input: Id,
        _revision: Revision,
        _cycle_heads: &mut CycleHeads,
    ) -> VerifyResult {
        // Input ingredients are just a counter, they store no data, they are immortal.
        // Their *fields* are stored in function ingredients elsewhere.
        VerifyResult::unchanged()
    }

    fn debug_name(&self) -> &'static str {
        C::DEBUG_NAME
    }

    fn memo_table_types(&self) -> &Arc<MemoTableTypes> {
        &self.memo_table_types
    }

    fn memo_table_types_mut(&mut self) -> &mut Arc<MemoTableTypes> {
        &mut self.memo_table_types
    }

    /// Returns memory usage information about any inputs.
    #[cfg(feature = "salsa_unstable")]
    fn memory_usage(&self, db: &dyn crate::Database) -> Option<Vec<crate::database::SlotInfo>> {
        let memory_usage = self
            .entries(db.zalsa())
            // SAFETY: The memo table belongs to a value that we allocated, so it
            // has the correct type.
            .map(|value| unsafe { value.memory_usage(&self.memo_table_types) })
            .collect();
        Some(memory_usage)
    }
}

impl<C: Configuration> std::fmt::Debug for IngredientImpl<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(std::any::type_name::<Self>())
            .field("index", &self.ingredient_index)
            .finish()
    }
}

#[derive(Debug)]
pub struct Value<C>
where
    C: Configuration,
{
    /// Fields of this input struct.
    ///
    /// They can change across revisions, but they do not change within
    /// a particular revision.
    fields: C::Fields,

    /// Revisions of the fields.
    revisions: C::Revisions,

    /// Durabilities of the fields.
    durabilities: C::Durabilities,

    /// Memos
    memos: MemoTable,
}

impl<C> Value<C>
where
    C: Configuration,
{
    /// Fields of this tracked struct.
    ///
    /// They can change across revisions, but they do not change within
    /// a particular revision.
    #[cfg(feature = "salsa_unstable")]
    pub fn fields(&self) -> &C::Fields {
        &self.fields
    }

    /// Returns memory usage information about the input.
    ///
    /// # Safety
    ///
    /// The `MemoTable` must belong to a `Value` of the correct type.
    #[cfg(feature = "salsa_unstable")]
    unsafe fn memory_usage(&self, memo_table_types: &MemoTableTypes) -> crate::database::SlotInfo {
        // SAFETY: The caller guarantees this is the correct types table.
        let memos = unsafe { memo_table_types.attach_memos(&self.memos) };

        crate::database::SlotInfo {
            debug_name: C::DEBUG_NAME,
            size_of_metadata: std::mem::size_of::<Self>() - std::mem::size_of::<C::Fields>(),
            size_of_fields: std::mem::size_of::<C::Fields>(),
            memos: memos.memory_usage(),
        }
    }
}

pub trait HasBuilder {
    type Builder;
}

// SAFETY: `Value<C>` is our private type branded over the unique configuration `C`.
unsafe impl<C> Slot for Value<C>
where
    C: Configuration,
{
    #[inline(always)]
    unsafe fn memos(&self, _current_revision: Revision) -> &crate::table::memo::MemoTable {
        &self.memos
    }

    #[inline(always)]
    fn memos_mut(&mut self) -> &mut crate::table::memo::MemoTable {
        &mut self.memos
    }
}
