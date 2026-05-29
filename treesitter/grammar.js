module.exports = grammar({
  name: 'sand',

  extras: $ => [
    /\s/,
    $.comment,
  ],

  word: $ => $.identifier,

  conflicts: $ => [
    [$.constructor_expr, $.primary],
  ],

  rules: {
    program: $ => repeat(choice(
      $.function_definition,
      $.type_alias,
      $.module_declaration,
    )),

    module_declaration: $ => seq(
      'module',
      field('name', $.identifier),
      optional(';')
    ),

    // ========= Enum type declarations =========
    type_alias: $ => seq(
      'type',
      field('name', $.identifier),
      '=',
      field('variant', $.identifier),
      repeat(seq('|', field('variant', $.identifier))),
      optional(';')
    ),

    // ========= Lexical =========
    comment: _ => token(choice(
      seq('/*', /[\s\S]*?/, '*/'),
      seq('//', /[^\n]*/)
    )),

    identifier: _ => /[a-zA-Z][a-zA-Z0-9_]*/,

    number: _ => /\d+/,

    boolean: _ => choice('true', 'false'),

    // ========= Types =========
    // Qualified cross-module type: mod::TypeName
    qualified_type: $ => seq(
      field('module', $.identifier),
      '::',
      field('name', $.identifier)
    ),

    // Ad-hoc structural tag union: #ok | #err | #pending
    tag_type: $ => seq(
      '#', field('tag', $.identifier),
      repeat(seq('|', '#', field('tag', $.identifier)))
    ),

    // All type forms unified under one inline rule
    _type: $ => choice(
      'Int',
      'Bool',
      'Unit',
      $.qualified_type,
      $.tag_type,
      $.identifier   // named enum type
    ),

    // ========= Functions =========
    function_definition: $ => seq(
      'def',
      field('name', $.identifier),
      '(',
      optional($.parameters),
      ')',
      ':',
      field('return_type', $._type),
      ':=',
      field('body', $._expression)
    ),

    parameters: $ => seq(
      $.parameter,
      repeat(seq(',', $.parameter)),
      optional(',')
    ),

    parameter: $ => seq(
      optional('mut'),
      field('name', choice($.identifier, '_')),
      ':',
      field('type', $._type)
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

    // Type annotation is optional: let x = ... or let x: Int = ...
    declaration: $ => seq(
      'let',
      optional('mut'),
      field('name', choice($.identifier, '_')),
      optional(seq(':', field('type', $._type))),
      '=',
      field('value', $._expression)
    ),

    assignment: $ => seq(
      field('name', $.identifier),
      '=',
      field('value', $._expression)
    ),

    // ========= Expressions =========
    _expression: $ => choice(
      $.logic_or,
      $.if_expression,
      $.while_expression,
      $.match_expression
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
      $.wildcard_pattern
    ),

    constructor_pattern: $ => seq(
      field('type_name', $.identifier),
      '#',
      field('variant', $.identifier)
    ),

    tag_pattern: $ => seq(
      '#',
      field('tag', $.identifier)
    ),

    wildcard_pattern: _ => '_',

    // ========= Block =========
    block: $ => seq(
      '{',
      repeat($.statement),
      optional($._expression),
      '}'
    ),

    // ========= Constructors & calls =========
    external_constructor_expr: $ => seq(
      field('module', $.identifier),
      '::',
      field('type_name', $.identifier),
      '#',
      field('variant', $.identifier)
    ),

    // mod::fn(args)
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
      field('variant', $.identifier)
    ),

    tag_expr: $ => seq(
      '#',
      field('variant', $.identifier)
    ),

    function_call: $ => seq(
      field('function', $.identifier),
      '(',
      optional(seq(
        $._expression,
        repeat(seq(',', $._expression))
      )),
      ')'
    ),

    // ========= Primary =========
    // Order matters for conflict resolution:
    //   external_constructor_expr before external_function_call (same prefix)
    //   constructor_expr before function_call and bare identifier (same prefix)
    primary: $ => choice(
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

    logic_and: $ => prec.left(3, seq(
      $.equality,
      repeat(seq('&', $.equality))
    )),

    logic_xor: $ => prec.left(2, seq(
      $.logic_and,
      repeat(seq(choice('⊕', '¡'), $.logic_and))
    )),

    logic_or: $ => prec.left(1, seq(
      $.logic_xor,
      repeat(seq('|', $.logic_xor))
    ))
  }
});
