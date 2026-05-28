module.exports = grammar({
  name: 'sand',

  extras: $ => [
    /\s/,
    $.comment,
  ],

  word: $ => $.identifier,

  conflicts: $ => [],

  rules: {
    program: $ => repeat(choice(
        $.function_definition,
        $.module_declaration
    )),

    module_declaration: $ => seq(
        'module',
        field('name', $.identifier),
        optional(';')
    ),

    // ========= Lexical =========
    comment: _ => token(choice(
      seq('/*', /[\s\S]*?/, '*/'),
      seq('//', /[^\n]*/)
    )),

    identifier: $ => /[a-zA-Z][a-zA-Z0-9_]*/,

    number: _ => /\d+/,

    boolean: _ => choice('true', 'false'),

    type: _ => choice('Int', 'Bool', 'Unit'),

    // ========= Functions =========
    function_definition: $ => seq(
      'def',
      field('name', $.identifier),
      '(',
      optional($.parameters),
      ')',
      ':',
      field('return_type', $.type),
      ':=',
      field('body', $._expression)
    ),

    parameters: $ => seq(
      $.parameter,
      repeat(seq(',', $.parameter)),
      optional(',')
    ),

    parameter: $ => seq(
      field('name', $.identifier),
      ':',
      field('type', $.type)
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

    declaration: $ => seq(
      'let',
      field('name', $.identifier),
      ':',
      field('type', $.type),
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
      $.while_expression
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

    block: $ => seq(
      '{',
      repeat($.statement),
      optional($._expression),
      '}'
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

    // The core units of expressions
    primary: $ => choice(
      seq('(', $._expression, ')'),
      $.function_call,
      $.number,
      $.boolean,
      $.identifier,
      $.block
    ),

    // ========= Operator precedence =========
    // We start from the highest precedence (unary) down to logic_or
    
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
      repeat(seq(choice('⊕', '#'), $.logic_and))
    )),

    logic_or: $ => prec.left(1, seq(
      $.logic_xor, 
      repeat(seq('|', $.logic_xor))
    ))
  }
});
