use crate::salsa_struct::{SalsaField, SalsaStruct};
use proc_macro2::{Literal, TokenStream};

/// For an entity struct `Foo` with fields `f1: T1, ..., fN: TN`, we generate...
///
/// * the "id struct" `struct Foo(salsa::Id)`
/// * the entity ingredient, which maps the id fields to the `Id`
/// * for each value field, a function ingredient
pub(crate) fn input(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    match SalsaStruct::new(args, input, "input").and_then(|el| InputStruct(el).generate_input()) {
        Ok(s) => s.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

struct InputStruct(SalsaStruct<Self>);

impl std::ops::Deref for InputStruct {
    type Target = SalsaStruct<Self>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl crate::options::AllowedOptions for InputStruct {
    const RETURN_REF: bool = false;

    const SPECIFY: bool = false;

    const NO_EQ: bool = false;
    const SINGLETON: bool = true;

    const JAR: bool = true;

    const DATA: bool = true;

    const DB: bool = false;

    const RECOVERY_FN: bool = false;

    const LRU: bool = false;

    const CONSTRUCTOR_NAME: bool = true;
}

impl InputStruct {
    fn generate_input(&self) -> syn::Result<TokenStream> {
        self.require_no_generics()?;

        let id_struct = self.the_struct_id();
        let inherent_impl = self.input_inherent_impl();
        let ingredients_for_impl = self.input_ingredients();
        let as_id_impl = self.as_id_impl();
        let from_id_impl = self.impl_of_from_id();
        let salsa_struct_in_db_impl = self.salsa_struct_in_db_impl();
        let as_debug_with_db_impl = self.as_debug_with_db_impl();
        let debug_impl = self.debug_impl();

        Ok(quote! {
            #id_struct
            #inherent_impl
            #ingredients_for_impl
            #as_id_impl
            #from_id_impl
            #as_debug_with_db_impl
            #salsa_struct_in_db_impl
            #debug_impl
        })
    }

    /// Generate an inherent impl with methods on the entity type.
    fn input_inherent_impl(&self) -> syn::ItemImpl {
        let ident = self.the_ident();
        let jar_ty = self.jar_ty();
        let db_dyn_ty = self.db_dyn_ty();
        let input_index = self.input_index();

        let field_indices = self.all_field_indices();
        let field_names = self.all_field_names();
        let field_vises = self.all_field_vises();
        let field_tys: Vec<_> = self.all_field_tys();
        let field_clones: Vec<_> = self.all_fields().map(SalsaField::is_clone_field).collect();
        let get_field_names: Vec<_> = self.all_get_field_names();
        let field_getters: Vec<syn::ImplItemFn> = field_indices.iter().zip(&get_field_names).zip(&field_vises).zip(&field_tys).zip(&field_clones).map(|((((field_index, get_field_name), field_vis), field_ty), is_clone_field)|
            if !*is_clone_field {
                parse_quote_spanned! { get_field_name.span() =>
                    #field_vis fn #get_field_name<'db>(self, __db: &'db #db_dyn_ty) -> &'db #field_ty
                    {
                        let (__jar, __runtime) = <_ as salsa::storage::HasJar<#jar_ty>>::jar(__db);
                        let __ingredients = <#jar_ty as salsa::storage::HasIngredientsFor< #ident >>::ingredient(__jar);
                        __ingredients.#field_index.fetch(__runtime, self)
                    }
                }
            } else {
                parse_quote_spanned! { get_field_name.span() =>
                    #field_vis fn #get_field_name<'db>(self, __db: &'db #db_dyn_ty) -> #field_ty
                    {
                        let (__jar, __runtime) = <_ as salsa::storage::HasJar<#jar_ty>>::jar(__db);
                        let __ingredients = <#jar_ty as salsa::storage::HasIngredientsFor< #ident >>::ingredient(__jar);
                        __ingredients.#field_index.fetch(__runtime, self).clone()
                    }
                }
            }
        )
        .collect();

        // setters
        let set_field_names = self.all_set_field_names();
        let field_setters: Vec<syn::ImplItemFn> = field_indices.iter()
            .zip(&set_field_names)
            .zip(&field_vises)
            .zip(&field_tys)
            .filter_map(|(((field_index, &set_field_name), field_vis), field_ty)| {
                let set_field_name = set_field_name?;
                Some(parse_quote_spanned! { set_field_name.span() =>
                    #field_vis fn #set_field_name<'db>(self, __db: &'db mut #db_dyn_ty) -> salsa::setter::Setter<'db, #ident, #field_ty>
                    {
                        let (__jar, __runtime) = <_ as salsa::storage::HasJar<#jar_ty>>::jar_mut(__db);
                        let __ingredients = <#jar_ty as salsa::storage::HasIngredientsFor< #ident >>::ingredient_mut(__jar);
                        salsa::setter::Setter::new(__runtime, self, &mut __ingredients.#field_index)
                    }
                })
        })
        .collect();

        let constructor_name = self.constructor_name();
        let singleton = self.0.is_isingleton();

        let constructor: syn::ImplItemFn = if singleton {
            parse_quote_spanned! { constructor_name.span() =>
                /// Creates a new singleton input
                ///
                /// # Panics
                ///
                /// If called when an instance already exists
                pub fn #constructor_name(__db: &#db_dyn_ty, #(#field_names: #field_tys,)*) -> Self
                {
                    let (__jar, __runtime) = <_ as salsa::storage::HasJar<#jar_ty>>::jar(__db);
                    let __ingredients = <#jar_ty as salsa::storage::HasIngredientsFor< #ident >>::ingredient(__jar);
                    let __id = __ingredients.#input_index.new_singleton_input(__runtime);
                    #(
                        __ingredients.#field_indices.store_new(__runtime, __id, #field_names, salsa::Durability::LOW);
                    )*
                    __id
                }
            }
        } else {
            parse_quote_spanned! { constructor_name.span() =>
                pub fn #constructor_name(__db: &#db_dyn_ty, #(#field_names: #field_tys,)*) -> Self
                {
                    let (__jar, __runtime) = <_ as salsa::storage::HasJar<#jar_ty>>::jar(__db);
                    let __ingredients = <#jar_ty as salsa::storage::HasIngredientsFor< #ident >>::ingredient(__jar);
                    let __id = __ingredients.#input_index.new_input(__runtime);
                    #(
                        __ingredients.#field_indices.store_new(__runtime, __id, #field_names, salsa::Durability::LOW);
                    )*
                    __id
                }
            }
        };

        let salsa_id = quote!(
            pub fn salsa_id(&self) -> salsa::Id {
                self.0
            }
        );

        if singleton {
            let get: syn::ImplItemFn = parse_quote! {
                #[track_caller]
                pub fn get(__db: &#db_dyn_ty) -> Self {
                    let (__jar, __runtime) = <_ as salsa::storage::HasJar<#jar_ty>>::jar(__db);
                    let __ingredients = <#jar_ty as salsa::storage::HasIngredientsFor< #ident >>::ingredient(__jar);
                    __ingredients.#input_index.get_singleton_input(__runtime).expect("singleton input struct not yet initialized")
                }
            };

            let try_get: syn::ImplItemFn = parse_quote! {
                #[track_caller]
                pub fn try_get(__db: &#db_dyn_ty) -> Option<Self> {
                    let (__jar, __runtime) = <_ as salsa::storage::HasJar<#jar_ty>>::jar(__db);
                    let __ingredients = <#jar_ty as salsa::storage::HasIngredientsFor< #ident >>::ingredient(__jar);
                    __ingredients.#input_index.get_singleton_input(__runtime)
                }
            };

            parse_quote! {
                #[allow(dead_code)]
                impl #ident {
                    #constructor

                    #get

                    #try_get

                    #(#field_getters)*

                    #(#field_setters)*

                    #salsa_id
                }
            }
        } else {
            parse_quote! {
                #[allow(dead_code, clippy::pedantic, clippy::complexity, clippy::style)]
                impl #ident {
                    #constructor

                    #(#field_getters)*

                    #(#field_setters)*

                    #salsa_id
                }
            }
        }

        // }
    }

    /// Generate the `IngredientsFor` impl for this entity.
    ///
    /// The entity's ingredients include both the main entity ingredient along with a
    /// function ingredient for each of the value fields.
    fn input_ingredients(&self) -> syn::ItemImpl {
        use crate::literal;
        let ident = self.the_ident();
        let field_ty = self.all_field_tys();
        let jar_ty = self.jar_ty();
        let all_field_indices: Vec<Literal> = self.all_field_indices();
        let input_index: Literal = self.input_index();
        let debug_name_struct = literal(self.the_ident());
        let debug_name_fields: Vec<_> = self.all_field_names().into_iter().map(literal).collect();

        parse_quote! {
            impl salsa::storage::IngredientsFor for #ident {
                type Jar = #jar_ty;
                type Ingredients = (
                    #(
                        salsa::input_field::InputFieldIngredient<#ident, #field_ty>,
                    )*
                    salsa::input::InputIngredient<#ident>,
                );

                fn create_ingredients<DB>(
                    routes: &mut salsa::routes::Routes<DB>,
                ) -> Self::Ingredients
                where
                    DB: salsa::DbWithJar<Self::Jar> + salsa::storage::JarFromJars<Self::Jar>,
                {
                    (
                        #(
                            {
                                let index = routes.push(
                                    |jars| {
                                        let jar = <DB as salsa::storage::JarFromJars<Self::Jar>>::jar_from_jars(jars);
                                        let ingredients = <_ as salsa::storage::HasIngredientsFor<Self>>::ingredient(jar);
                                        &ingredients.#all_field_indices
                                    },
                                    |jars| {
                                        let jar = <DB as salsa::storage::JarFromJars<Self::Jar>>::jar_from_jars_mut(jars);
                                        let ingredients = <_ as salsa::storage::HasIngredientsFor<Self>>::ingredient_mut(jar);
                                        &mut ingredients.#all_field_indices
                                    },
                                );
                                salsa::input_field::InputFieldIngredient::new(index, #debug_name_fields)
                            },
                        )*
                        {
                            let index = routes.push(
                                |jars| {
                                    let jar = <DB as salsa::storage::JarFromJars<Self::Jar>>::jar_from_jars(jars);
                                    let ingredients = <_ as salsa::storage::HasIngredientsFor<Self>>::ingredient(jar);
                                    &ingredients.#input_index
                                },
                                |jars| {
                                    let jar = <DB as salsa::storage::JarFromJars<Self::Jar>>::jar_from_jars_mut(jars);
                                    let ingredients = <_ as salsa::storage::HasIngredientsFor<Self>>::ingredient_mut(jar);
                                    &mut ingredients.#input_index
                                },
                            );
                            salsa::input::InputIngredient::new(index, #debug_name_struct)
                        },
                    )
                }
            }
        }
    }

    /// For the entity, we create a tuple that contains the function ingredients
    /// for each "other" field and the entity ingredient. This is the index of
    /// the entity ingredient within that tuple.
    fn input_index(&self) -> Literal {
        Literal::usize_unsuffixed(self.all_fields().count())
    }

    /// For the entity, we create a tuple that contains the function ingredients
    /// for each field and an entity ingredient. These are the indices
    /// of the function ingredients within that tuple.
    fn all_field_indices(&self) -> Vec<Literal> {
        self.all_fields()
            .zip(0..)
            .map(|(_, i)| Literal::usize_unsuffixed(i))
            .collect()
    }

    /// Names of setters of all fields that should be generated. Returns an optional Ident for the field name
    /// that is None when the field should not generate a setter.
    ///
    /// Setters are not created for fields with #[id] tag so they'll be safe to include in debug formatting
    pub(crate) fn all_set_field_names(&self) -> Vec<Option<&syn::Ident>> {
        self.all_fields()
            .map(|ef| (!ef.has_id_attr).then(|| ef.set_name()))
            .collect()
    }

    /// Implementation of `SalsaStructInDb`.
    fn salsa_struct_in_db_impl(&self) -> syn::ItemImpl {
        let ident = self.the_ident();
        let jar_ty = self.jar_ty();
        parse_quote! {
            impl<DB> salsa::salsa_struct::SalsaStructInDb<DB> for #ident
            where
                DB: ?Sized + salsa::DbWithJar<#jar_ty>,
            {
                fn register_dependent_fn(_db: &DB, _index: salsa::routes::IngredientIndex) {
                    // Do nothing here, at least for now.
                    // If/when we add ability to delete inputs, this would become relevant.
                }
            }
        }
    }
}