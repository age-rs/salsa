# Summary

- [About salsa](./about_salsa.md)

# How to use Salsa

- [Overview](./overview.md)
- [Tutorial: calc language](./tutorial.md)
  - [Basic structure](./tutorial/structure.md)
  - [Defining the database struct](./tutorial/db.md)
  - [Defining the IR: the various "salsa structs"](./tutorial/ir.md)
  - [Defining the parser: memoized functions and inputs](./tutorial/parser.md)
  - [Defining the parser: reporting errors](./tutorial/accumulators.md)
  - [Defining the parser: debug impls and testing](./tutorial/debug.md)
  - [Defining the checker](./tutorial/checker.md)
  - [Defining the interpreter](./tutorial/interpreter.md)
- [Reference](./reference.md)
  - [Durability](./reference/durability.md)
  - [Algorithm](./reference/algorithm.md)
- [Common patterns](./common_patterns.md)
  - [On-demand (Lazy) inputs](./common_patterns/on_demand_inputs.md)
- [Tuning](./tuning.md)
- [Cycle handling](./cycles.md)

# How Salsa works internally

- [How Salsa works](./how_salsa_works.md)
- [Videos](./videos.md)
- [Plumbing](./plumbing.md)
  - [Databases and runtime](./plumbing/database_and_runtime.md)
  - [The db lifetime on tracked/interned structs](./plumbing/db_lifetime.md)
  - [Tracked structures](./plumbing/tracked_structs.md)
  - [Query operations](./plumbing/query_ops.md)
    - [maybe changed after](./plumbing/maybe_changed_after.md)
    - [Fetch](./plumbing/fetch.md)
    - [Derived queries flowchart](./plumbing/derived_flowchart.md)
    - [Cycle handling](./plumbing/cycles.md)
  - [Terminology](./plumbing/terminology.md)
    - [Backdate](./plumbing/terminology/backdate.md)
    - [Changed at](./plumbing/terminology/changed_at.md)
    - [Dependency](./plumbing/terminology/dependency.md)
    - [Derived query](./plumbing/terminology/derived_query.md)
    - [Durability](./plumbing/terminology/durability.md)
    - [Input query](./plumbing/terminology/input_query.md)
    - [Ingredient](./plumbing/terminology/ingredient.md)
    - [LRU](./plumbing/terminology/LRU.md)
    - [Memo](./plumbing/terminology/memo.md)
    - [Query](./plumbing/terminology/query.md)
    - [Query function](./plumbing/terminology/query_function.md)
    - [Revision](./plumbing/terminology/revision.md)
    - [Salsa item](./plumbing/terminology/salsa_item.md)
    - [Salsa struct](./plumbing/terminology/salsa_struct.md)
    - [Untracked dependency](./plumbing/terminology/untracked.md)
    - [Verified](./plumbing/terminology/verified.md)

# Appendices

- [Meta: about the book itself](./meta.md)
