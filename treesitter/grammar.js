module.exports = grammar({
  name: 'sand',

  extras: $ => [
    /\s/,
    $.comment,
  ],

  word: $ => $.identifier,

  conflicts: $ => [
    [$.function_call, $.primary],
    [$.constructor_expr, $.primary],
    // self-conflicts: the optional payload `(…)` after a `#variant` must be
    // attached greedily (GLR decides) rather than left dangling.
    [$.constructor_expr],
    [$.tag_expr],
    [$.external_constructor_expr],
  ],

  rules: {
    program: $ => repeat(choice(
      $.function_definition,
      $.extern_declaration,
      $.type_alias,
      $.typeclass_declaration,
      $.impl_declaration,
      $.module_declaration,
      $.use_declaration,
    )),

    // ========= Top-level: modules & imports =========
    module_declaration: $ => seq(
      'module',
      field('name', $.identifier),
      optional(';')
    ),

    use_declaration: $ => seq(
      'use',
      $.use_path,
      ';'
    ),

    use_path: $ => seq(
      $.identifier,
      repeat(seq('::', $.identifier)),
      optional(seq('::', $.use_glob))
    ),

    use_glob: _ => '*',

    // ========= Lexical =========
    comment: _ => token(choice(
      seq('/*', /[\s\S]*?/, '*/'),
      seq('//', /[^\n]*/)
    )),

    identifier: _ => /_*[a-zA-Z][a-zA-Z0-9_]*/,

    // `_` placeholder (parameters, let-bindings)
    empty_identifier: _ => '_',

    number: _ => /\d+/,

    boolean: _ => choice('true', 'false'),

    // a lifetime / region: `'r`, `'static`
    lifetime: _ => token(seq("'", /[a-zA-Z][a-zA-Z0-9_]*/)),

    // ========= Generics (declaration position) =========
    // <T>, <+a : Owned, -b, 'r>
    type_params: $ => seq(
      '<',
      choice($.type_param, $.region_param),
      repeat(seq(',', choice($.type_param, $.region_param))),
      '>'
    ),

    type_param: $ => seq(
      optional($.variance_ann),
      field('name', $.identifier),
      optional(seq(':', field('kind', $.kind_ann)))
    ),

    region_param: $ => $.lifetime,

    variance_ann: _ => choice('+', '-'),

    // a kind: `Owned`, `Never`, or an arrow `Owned -> Owned` (right-assoc)
    kind_ann: $ => prec.right(seq(
      $.kind_atom,
      repeat(seq('->', $.kind_atom))
    )),

    kind_atom: $ => choice(
      'Owned',
      'Never',
      seq('(', $.kind_ann, ')')
    ),

    // where 'r >= 's, T : C
    where_clause: $ => seq(
      'where',
      $.where_constraint,
      repeat(seq(',', $.where_constraint))
    ),

    where_constraint: $ => choice(
      seq($.lifetime, '>=', $.lifetime),
      seq(field('param', $.identifier), ':', field('class', $.identifier))
    ),

    // ========= Types =========
    qualified_type: $ => seq(
      field('module', $.identifier),
      '::',
      field('name', $.identifier)
    ),

    // Ad-hoc structural tag union: #ok | #err
    tag_type: $ => seq(
      '#', field('tag', $.identifier),
      repeat(seq('|', '#', field('tag', $.identifier)))
    ),

    // Tuple type: (Int, Bool) arity >= 2
    tuple_type: $ => seq(
      '(',
      $._type,
      repeat1(seq(',', $._type)),
      ')'
    ),

    // Generic instantiation: Option<Int>, Holder<'a, Int>
    type_application: $ => seq(
      field('name', $.identifier),
      '<',
      $.type_app_arg,
      repeat(seq(',', $.type_app_arg)),
      '>'
    ),

    type_app_arg: $ => choice($.lifetime, $._type),

    // reference type: &T, &'r T, &'r mut T
    borrow_type: $ => seq(
      '&',
      optional($.lifetime),
      optional('mut'),
      $._core_type
    ),

    // `->` (reusable) or `-[Owned|BorrowedMut|Borrowed]>` (the calling-mode arrow).
    fn_arrow: $ => choice(
      '->',
      seq('-[', $.arrow_kind, ']>')
    ),

    arrow_kind: _ => choice('Owned', 'BorrowedMut', 'Borrowed'),

    // a function type `A -> B`, right-associative; domain is a core type.
    function_type: $ => prec.right(seq(
      field('param', $._core_type),
      $.fn_arrow,
      field('return', $._type)
    )),

    // all non-function, non-ascribed type forms.
    _core_type: $ => choice(
      'Int',
      'Bool',
      'Unit',
      $.qualified_type,
      $.tag_type,
      $.tuple_type,
      $.type_application,
      $.borrow_type,
      $.identifier   // named enum type
    ),

    // a type, optionally ascribed to a region: `Int @ 'r`
    _type: $ => choice(
      $.function_type,
      $.region_ascription,
      $._core_type
    ),

    region_ascription: $ => seq($._core_type, '@', $.lifetime),

    // ========= Enum type declarations =========
    // Multiple comma-separated payload types desugar to a single tuple payload.
    enum_variant: $ => seq(
      field('name', $.identifier),
      optional(seq(
        '(',
        field('payload_type', $._type),
        repeat(seq(',', field('payload_type', $._type))),
        ')'
      ))
    ),

    deriving_clause: $ => seq(
      'deriving',
      field('class', $.identifier),
      repeat(seq(',', field('class', $.identifier)))
    ),

    type_alias: $ => seq(
      'type',
      field('name', $.identifier),
      optional($.type_params),
      '=',
      field('variant', $.enum_variant),
      repeat(seq('|', field('variant', $.enum_variant))),
      optional($.deriving_clause),
      optional(';')
    ),

    // ========= Functions =========
    function_definition: $ => seq(
      'def',
      field('name', $.identifier),
      optional($.type_params),
      '(',
      optional($.parameters),
      ')',
      ':',
      field('return_type', $._type),
      optional($.where_clause),
      ':=',
      field('body', $._expression)
    ),

    // External (FFI) declaration: a bodyless `def`.
    extern_declaration: $ => seq(
      'extern',
      'def',
      field('name', $.identifier),
      '(',
      optional($.parameters),
      ')',
      ':',
      field('return_type', $._type),
      ';'
    ),

    parameters: $ => seq(
      $.parameter,
      repeat(seq(',', $.parameter)),
      optional(',')
    ),

    parameter: $ => seq(
      optional('mut'),
      field('name', choice($.identifier, $.empty_identifier)),
      ':',
      field('type', $._type)
    ),

    // ========= Typeclasses & impls =========
    typeclass_declaration: $ => seq(
      'typeclass',
      field('name', $.identifier),
      $.type_params,
      optional($.requires_clause),
      '{',
      repeat($.typeclass_method),
      '}'
    ),

    // a method signature, with an optional default body.
    typeclass_method: $ => seq(
      'def',
      field('name', $.identifier),
      optional($.type_params),
      '(',
      optional($.parameters),
      ')',
      ':',
      field('return_type', $._type),
      optional($.where_clause),
      optional(seq(':=', field('body', $._expression)))
    ),

    requires_clause: $ => seq(
      'requires',
      field('class', $.identifier),
      repeat(seq(',', field('class', $.identifier)))
    ),

    impl_declaration: $ => seq(
      'impl',
      field('class', $.identifier),
      'for',
      field('type', $._type),
      '{',
      repeat($.function_definition),
      '}'
    ),

    // ========= Statements =========
    statement: $ => seq(
      choice(
        $.declaration,
        $.assignment,
        $._expression
      ),
      ';'
    ),

    // let-binding LHS forms: a plain (optionally mut) binding, a tuple pattern, a
    // borrow binding, or a constructor pattern (the latter needs an `else`).
    declaration: $ => seq(
      'let',
      choice(
        $.let_constructor,
        $.let_tuple,
        $.borrow_binding,
        seq(optional('mut'), field('name', choice($.identifier, $.empty_identifier)))
      ),
      optional(seq(':', field('type', $._type))),
      '=',
      field('value', $._expression),
      optional(seq('else', field('fallback', $._expression)))
    ),

    let_tuple: $ => seq(
      '(',
      $.let_tuple_elem,
      repeat1(seq(',', $.let_tuple_elem)),
      ')'
    ),

    let_tuple_elem: $ => seq(optional('mut'), $.identifier),

    let_binding_elem: $ => choice($.identifier, $.empty_identifier),

    let_binding_tuple: $ => seq(
      '(',
      $.let_binding_elem,
      repeat1(seq(',', $.let_binding_elem)),
      ')'
    ),

    let_destructure: $ => choice(
      $.let_constructor,
      $.let_binding_tuple,
      $.let_binding_elem
    ),

    let_constructor: $ => seq(
      field('type_name', $.identifier),
      '#',
      field('variant', $.identifier),
      optional(seq('(', $.let_destructure, ')'))
    ),

    borrow_binding: $ => seq(
      '&',
      optional('mut'),
      choice($.identifier, $.empty_identifier)
    ),

    assignment: $ => seq(
      field('target', choice($.deref_expr, $.identifier)),
      '=',
      field('value', $._expression)
    ),

    // ========= Expressions =========
    _expression: $ => choice(
      $.lambda_expr,
      $.logic_or,
      $.if_expression,
      $.while_expression,
      $.match_expression
    ),

    // Lambda value: `fn (x: T) -> e` or `fn (x: T) -[Owned]> e`.
    lambda_expr: $ => prec.right(seq(
      'fn',
      $.lambda_param,
      $.fn_arrow,
      field('body', $._expression)
    )),

    lambda_param: $ => seq(
      '(',
      optional('mut'),
      field('param', $.identifier),
      ':',
      field('param_type', $._type),
      ')'
    ),

    if_expression: $ => prec.right(seq(
      'if',
      field('condition', $._expression),
      'then',
      field('then_branch', $._expression),
      optional(seq(
        'else',
        field('else_branch', $._expression)
      ))
    )),

    while_expression: $ => seq(
      'while',
      field('condition', $._expression),
      'do',
      field('body', $._expression)
    ),

    // ========= Pattern matching =========
    match_expression: $ => seq(
      'match',
      field('scrutinee', $._expression),
      '{',
      repeat1($.match_arm),
      '}'
    ),

    match_arm: $ => seq(
      field('pattern', $.pattern),
      '=>',
      field('body', $._expression),
      optional(',')
    ),

    pattern: $ => choice(
      $.constructor_pattern,
      $.tag_pattern,
      $.tuple_pattern,
      $.bool_literal_pattern,
      $.int_literal_pattern,
      $.binding_pattern,
      $.wildcard_pattern
    ),

    constructor_pattern: $ => seq(
      field('type_name', $.identifier),
      '#',
      field('variant', $.identifier),
      optional(seq(
        '(',
        field('pattern', $.pattern),
        repeat(seq(',', field('pattern', $.pattern))),
        ')'
      ))
    ),

    tag_pattern: $ => seq(
      '#',
      field('tag', $.identifier),
      optional(seq(
        '(',
        field('pattern', $.pattern),
        repeat(seq(',', field('pattern', $.pattern))),
        ')'
      ))
    ),

    tuple_pattern: $ => seq(
      '(',
      field('pattern', $.pattern),
      repeat1(seq(',', field('pattern', $.pattern))),
      ')'
    ),

    int_literal_pattern: _ => token(seq(optional('-'), /\d+/)),

    bool_literal_pattern: _ => choice('true', 'false'),

    binding_pattern: $ => $.identifier,

    wildcard_pattern: _ => '_',

    // ========= Constructors & calls =========
    // `f::<T>(args)`
    turbofish: $ => seq(
      '::',
      '<',
      $._type,
      repeat(seq(',', $._type)),
      '>'
    ),

    external_constructor_expr: $ => seq(
      field('module', $.identifier),
      '::',
      field('type_name', $.identifier),
      '#',
      field('variant', $.identifier),
      optional(seq(
        '(',
        field('payload', $._expression),
        repeat(seq(',', field('payload', $._expression))),
        ')'
      ))
    ),

    external_function_call: $ => seq(
      field('module', $.identifier),
      '::',
      field('function', $.identifier),
      '(',
      optional(seq(
        $._expression,
        repeat(seq(',', $._expression))
      )),
      ')'
    ),

    constructor_expr: $ => seq(
      field('type_name', $.identifier),
      '#',
      field('variant', $.identifier),
      optional(seq(
        '(',
        field('payload', $._expression),
        repeat(seq(',', field('payload', $._expression))),
        ')'
      ))
    ),

    // Bare tag, optionally with payload(s): `#None`, `#Some(x)`, `#Cons(x, xs)`.
    tag_expr: $ => seq(
      '#',
      field('variant', $.identifier),
      optional(seq(
        '(',
        field('payload', $._expression),
        repeat(seq(',', field('payload', $._expression))),
        ')'
      ))
    ),

    function_call: $ => seq(
      field('function', $.identifier),
      optional($.turbofish),
      '(',
      optional(seq(
        $._expression,
        repeat(seq(',', $._expression))
      )),
      ')'
    ),

    // Tuple literal: (a, b, ...) arity >= 2
    tuple_expr: $ => seq(
      '(',
      $._expression,
      repeat1(seq(',', $._expression)),
      ')'
    ),

    // borrow `&e` / `&mut e` and dereference `*e`, each binding to a primary.
    borrow_expr: $ => prec(11, seq(
      '&',
      optional('mut'),
      $.primary
    )),

    deref_expr: $ => prec(11, seq('*', $.primary)),

    // ========= Block =========
    block: $ => seq(
      '{',
      repeat($.statement),
      optional($._expression),
      '}'
    ),

    // ========= Primary =========
    primary: $ => choice(
      $.borrow_expr,
      $.deref_expr,
      $.tuple_expr,
      seq('(', $._expression, ')'),
      $.external_constructor_expr,
      $.external_function_call,
      $.constructor_expr,
      $.function_call,
      $.tag_expr,
      $.number,
      $.boolean,
      $.identifier,
      $.block
    ),

    // ========= Operator precedence =========
    // Highest precedence (9) → lowest (1)
    unary: $ => choice(
      prec.right(9, seq(choice('-', '!'), $.unary)),
      $.primary
    ),

    power: $ => prec.right(8, seq(
      $.unary,
      optional(seq('^', $.power))
    )),

    multiplicative: $ => prec.left(7, seq(
      $.power,
      repeat(seq(choice('*', '/'), $.power))
    )),

    additive: $ => prec.left(6, seq(
      $.multiplicative,
      repeat(seq(choice('+', '-'), $.multiplicative))
    )),

    comparison: $ => prec.left(5, seq(
      $.additive,
      repeat(seq(choice('>', '<', '>=', '<=', '≥', '≤'), $.additive))
    )),

    equality: $ => prec.left(4, seq(
      $.comparison,
      repeat(seq(choice('==', '!=', '≠'), $.comparison))
    )),

    // Boolean operators are keywords (`and`/`or`/`xor`) or symbols (`&&`/`⊕`);
    // `|` is reserved for the enum/tag-union separator, not boolean-or.
    logic_and: $ => prec.left(3, seq(
      $.equality,
      repeat(seq(choice('and', '&&'), $.equality))
    )),

    logic_xor: $ => prec.left(2, seq(
      $.logic_and,
      repeat(seq(choice('xor', '⊕'), $.logic_and))
    )),

    logic_or: $ => prec.left(1, seq(
      $.logic_xor,
      repeat(seq('or', $.logic_xor))
    ))
  }
});
