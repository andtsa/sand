; ModuleID = 'sand_module'
source_filename = "sand_module"

@fmt_read_int = private unnamed_addr constant [4 x i8] c"%ld\00", align 1
@fmt_int = private unnamed_addr constant [5 x i8] c"%ld \00", align 1
@__enum_2_variant_0_name = private constant [4 x i8] c"#eq\00"
@__enum_2_variant_1_name = private constant [4 x i8] c"#gt\00"
@__enum_2_variant_2_name = private constant [4 x i8] c"#lt\00"
@__enum_2_variants = private constant [3 x ptr] [ptr @__enum_2_variant_0_name, ptr @__enum_2_variant_1_name, ptr @__enum_2_variant_2_name]
@fmt_enum = private unnamed_addr constant [4 x i8] c"%s \00", align 1
@fmt_int.1 = private unnamed_addr constant [5 x i8] c"%ld \00", align 1
@fmt_nl = private unnamed_addr constant [2 x i8] c"\0A\00", align 1
@fmt_int.2 = private unnamed_addr constant [5 x i8] c"%ld \00", align 1
@fmt_enum.3 = private unnamed_addr constant [4 x i8] c"%s \00", align 1
@fmt_int.4 = private unnamed_addr constant [5 x i8] c"%ld \00", align 1
@fmt_nl.5 = private unnamed_addr constant [2 x i8] c"\0A\00", align 1
@fmt_int.6 = private unnamed_addr constant [5 x i8] c"%ld \00", align 1
@fmt_enum.7 = private unnamed_addr constant [4 x i8] c"%s \00", align 1
@fmt_int.8 = private unnamed_addr constant [5 x i8] c"%ld \00", align 1
@fmt_nl.9 = private unnamed_addr constant [2 x i8] c"\0A\00", align 1
@__enum_0_variant_0_name = private constant [4 x i8] c"One\00"
@__enum_0_variant_1_name = private constant [4 x i8] c"Two\00"
@__enum_0_variant_2_name = private constant [6 x i8] c"Three\00"
@__enum_0_variants = private constant [3 x ptr] [ptr @__enum_0_variant_0_name, ptr @__enum_0_variant_1_name, ptr @__enum_0_variant_2_name]
@fmt_enum.10 = private unnamed_addr constant [4 x i8] c"%s \00", align 1
@fmt_nl.11 = private unnamed_addr constant [2 x i8] c"\0A\00", align 1
@fmt_enum.12 = private unnamed_addr constant [4 x i8] c"%s \00", align 1
@fmt_nl.13 = private unnamed_addr constant [2 x i8] c"\0A\00", align 1
@fmt_bool = private unnamed_addr constant [4 x i8] c"%d \00", align 1
@fmt_nl.14 = private unnamed_addr constant [2 x i8] c"\0A\00", align 1
@fmt_bool.15 = private unnamed_addr constant [4 x i8] c"%d \00", align 1
@fmt_nl.16 = private unnamed_addr constant [2 x i8] c"\0A\00", align 1

define i64 @abs(i64 %0) {
entry:
  %local = alloca i64, align 8
  %local1 = alloca i64, align 8
  %local2 = alloca i64, align 8
  store i64 %0, ptr %local, align 4
  br label %bb0

bb0:                                              ; preds = %entry
  %load = load i64, ptr %local, align 4
  store i64 %load, ptr %local2, align 4
  br label %bb1

bb1:                                              ; preds = %bb0
  %load3 = load i64, ptr %local2, align 4
  %abs = call i64 @llvm.abs.i64(i64 %load3, i1 false)
  store i64 %abs, ptr %local1, align 4
  br label %bb2

bb2:                                              ; preds = %bb1
  %load4 = load i64, ptr %local1, align 4
  ret i64 %load4
}

define i64 @min(i64 %0, i64 %1) {
entry:
  %local = alloca i64, align 8
  %local1 = alloca i64, align 8
  %local2 = alloca i64, align 8
  %local3 = alloca i64, align 8
  %local4 = alloca i64, align 8
  store i64 %0, ptr %local, align 4
  store i64 %1, ptr %local1, align 4
  br label %bb0

bb0:                                              ; preds = %entry
  %load = load i64, ptr %local, align 4
  store i64 %load, ptr %local3, align 4
  br label %bb1

bb1:                                              ; preds = %bb0
  %load5 = load i64, ptr %local1, align 4
  store i64 %load5, ptr %local4, align 4
  br label %bb2

bb2:                                              ; preds = %bb1
  %load6 = load i64, ptr %local3, align 4
  %load7 = load i64, ptr %local4, align 4
  %min = call i64 @llvm.smin.i64(i64 %load6, i64 %load7)
  store i64 %min, ptr %local2, align 4
  br label %bb3

bb3:                                              ; preds = %bb2
  %load8 = load i64, ptr %local2, align 4
  ret i64 %load8
}

define i64 @max(i64 %0, i64 %1) {
entry:
  %local = alloca i64, align 8
  %local1 = alloca i64, align 8
  %local2 = alloca i64, align 8
  %local3 = alloca i64, align 8
  %local4 = alloca i64, align 8
  store i64 %0, ptr %local, align 4
  store i64 %1, ptr %local1, align 4
  br label %bb0

bb0:                                              ; preds = %entry
  %load = load i64, ptr %local, align 4
  store i64 %load, ptr %local3, align 4
  br label %bb1

bb1:                                              ; preds = %bb0
  %load5 = load i64, ptr %local1, align 4
  store i64 %load5, ptr %local4, align 4
  br label %bb2

bb2:                                              ; preds = %bb1
  %load6 = load i64, ptr %local3, align 4
  %load7 = load i64, ptr %local4, align 4
  %max = call i64 @llvm.smax.i64(i64 %load6, i64 %load7)
  store i64 %max, ptr %local2, align 4
  br label %bb3

bb3:                                              ; preds = %bb2
  %load8 = load i64, ptr %local2, align 4
  ret i64 %load8
}

define i64 @clamp(i64 %0, i64 %1, i64 %2) {
entry:
  %local = alloca i64, align 8
  %local1 = alloca i64, align 8
  %local2 = alloca i64, align 8
  %local3 = alloca i64, align 8
  %local4 = alloca i64, align 8
  %local5 = alloca i64, align 8
  %local6 = alloca i64, align 8
  %local7 = alloca i64, align 8
  store i64 %0, ptr %local, align 4
  store i64 %1, ptr %local1, align 4
  store i64 %2, ptr %local2, align 4
  br label %bb0

bb0:                                              ; preds = %entry
  %load = load i64, ptr %local, align 4
  store i64 %load, ptr %local6, align 4
  br label %bb1

bb1:                                              ; preds = %bb0
  %load8 = load i64, ptr %local1, align 4
  store i64 %load8, ptr %local7, align 4
  br label %bb2

bb2:                                              ; preds = %bb1
  %load9 = load i64, ptr %local6, align 4
  %load10 = load i64, ptr %local7, align 4
  %call = call i64 @max(i64 %load9, i64 %load10)
  store i64 %call, ptr %local4, align 4
  br label %bb3

bb3:                                              ; preds = %bb2
  %load11 = load i64, ptr %local2, align 4
  store i64 %load11, ptr %local5, align 4
  br label %bb4

bb4:                                              ; preds = %bb3
  %load12 = load i64, ptr %local4, align 4
  %load13 = load i64, ptr %local5, align 4
  %call14 = call i64 @min(i64 %load12, i64 %load13)
  store i64 %call14, ptr %local3, align 4
  br label %bb5

bb5:                                              ; preds = %bb4
  %load15 = load i64, ptr %local3, align 4
  ret i64 %load15
}

define i1 @is_odd(i64 %0) {
entry:
  %local = alloca i64, align 8
  %local1 = alloca i1, align 1
  %local2 = alloca i64, align 8
  %local3 = alloca i64, align 8
  %local4 = alloca i64, align 8
  %local5 = alloca i64, align 8
  store i64 %0, ptr %local, align 4
  br label %bb0

bb0:                                              ; preds = %entry
  %load = load i64, ptr %local, align 4
  store i64 %load, ptr %local4, align 4
  br label %bb1

bb1:                                              ; preds = %bb0
  store i64 1, ptr %local5, align 4
  br label %bb2

bb2:                                              ; preds = %bb1
  %load6 = load i64, ptr %local4, align 4
  %load7 = load i64, ptr %local5, align 4
  %and = and i64 %load6, %load7
  store i64 %and, ptr %local2, align 4
  br label %bb3

bb3:                                              ; preds = %bb2
  store i64 0, ptr %local3, align 4
  br label %bb4

bb4:                                              ; preds = %bb3
  %load8 = load i64, ptr %local2, align 4
  %load9 = load i64, ptr %local3, align 4
  %cmp = icmp ne i64 %load8, %load9
  store i1 %cmp, ptr %local1, align 1
  br label %bb5

bb5:                                              ; preds = %bb4
  %load10 = load i1, ptr %local1, align 1
  ret i1 %load10
}

define i1 @is_even(i64 %0) {
entry:
  %local = alloca i64, align 8
  %local1 = alloca i1, align 1
  %local2 = alloca i64, align 8
  %local3 = alloca i64, align 8
  %local4 = alloca i64, align 8
  %local5 = alloca i64, align 8
  store i64 %0, ptr %local, align 4
  br label %bb0

bb0:                                              ; preds = %entry
  %load = load i64, ptr %local, align 4
  store i64 %load, ptr %local4, align 4
  br label %bb1

bb1:                                              ; preds = %bb0
  store i64 1, ptr %local5, align 4
  br label %bb2

bb2:                                              ; preds = %bb1
  %load6 = load i64, ptr %local4, align 4
  %load7 = load i64, ptr %local5, align 4
  %and = and i64 %load6, %load7
  store i64 %and, ptr %local2, align 4
  br label %bb3

bb3:                                              ; preds = %bb2
  store i64 0, ptr %local3, align 4
  br label %bb4

bb4:                                              ; preds = %bb3
  %load8 = load i64, ptr %local2, align 4
  %load9 = load i64, ptr %local3, align 4
  %cmp = icmp eq i64 %load8, %load9
  store i1 %cmp, ptr %local1, align 1
  br label %bb5

bb5:                                              ; preds = %bb4
  %load10 = load i1, ptr %local1, align 1
  ret i1 %load10
}

define i64 @pow(i64 %0, i64 %1) {
entry:
  %local = alloca i64, align 8
  %local1 = alloca i64, align 8
  %local2 = alloca i64, align 8
  %local3 = alloca i64, align 8
  %local4 = alloca i64, align 8
  %local5 = alloca i64, align 8
  %local6 = alloca i64, align 8
  %local7 = alloca i64, align 8
  %local8 = alloca i64, align 8
  %local9 = alloca i64, align 8
  %local10 = alloca i64, align 8
  %local11 = alloca i64, align 8
  %local12 = alloca i64, align 8
  %local13 = alloca i64, align 8
  %local14 = alloca i64, align 8
  %local15 = alloca i64, align 8
  %local16 = alloca i64, align 8
  %local17 = alloca i64, align 8
  %local18 = alloca i64, align 8
  %local19 = alloca i64, align 8
  %local20 = alloca i64, align 8
  %local21 = alloca i64, align 8
  %local22 = alloca i64, align 8
  %local23 = alloca i64, align 8
  %local24 = alloca i64, align 8
  %local25 = alloca i1, align 1
  %local26 = alloca i64, align 8
  %local27 = alloca i64, align 8
  %local28 = alloca i64, align 8
  %local29 = alloca i1, align 1
  %local30 = alloca i64, align 8
  %local31 = alloca i64, align 8
  %local32 = alloca i1, align 1
  %local33 = alloca i64, align 8
  %local34 = alloca i64, align 8
  %local35 = alloca i1, align 1
  %local36 = alloca i64, align 8
  %local37 = alloca i64, align 8
  %local38 = alloca i1, align 1
  %local39 = alloca i64, align 8
  %local40 = alloca i64, align 8
  %local41 = alloca i1, align 1
  store i64 %0, ptr %local, align 4
  store i64 %1, ptr %local1, align 4
  br label %bb0

bb0:                                              ; preds = %entry
  %load = load i64, ptr %local, align 4
  store i64 %load, ptr %local39, align 4
  br label %bb1

bb1:                                              ; preds = %bb0
  store i64 0, ptr %local40, align 4
  br label %bb2

bb2:                                              ; preds = %bb1
  %load42 = load i64, ptr %local39, align 4
  %load43 = load i64, ptr %local40, align 4
  %cmp = icmp eq i64 %load42, %load43
  store i1 %cmp, ptr %local41, align 1
  br label %bb3

bb3:                                              ; preds = %bb2
  %load44 = load i1, ptr %local41, align 1
  br i1 %load44, label %bb51, label %bb4

bb4:                                              ; preds = %bb3
  %load45 = load i64, ptr %local, align 4
  store i64 %load45, ptr %local36, align 4
  br label %bb5

bb5:                                              ; preds = %bb4
  store i64 1, ptr %local37, align 4
  br label %bb6

bb6:                                              ; preds = %bb5
  %load46 = load i64, ptr %local36, align 4
  %load47 = load i64, ptr %local37, align 4
  %cmp48 = icmp eq i64 %load46, %load47
  store i1 %cmp48, ptr %local38, align 1
  br label %bb7

bb7:                                              ; preds = %bb6
  %load49 = load i1, ptr %local38, align 1
  br i1 %load49, label %bb49, label %bb8

bb8:                                              ; preds = %bb7
  %load50 = load i64, ptr %local1, align 4
  store i64 %load50, ptr %local33, align 4
  br label %bb9

bb9:                                              ; preds = %bb8
  store i64 0, ptr %local34, align 4
  br label %bb10

bb10:                                             ; preds = %bb9
  %load51 = load i64, ptr %local33, align 4
  %load52 = load i64, ptr %local34, align 4
  %cmp53 = icmp slt i64 %load51, %load52
  store i1 %cmp53, ptr %local35, align 1
  br label %bb11

bb11:                                             ; preds = %bb10
  %load54 = load i1, ptr %local35, align 1
  br i1 %load54, label %bb47, label %bb12

bb12:                                             ; preds = %bb11
  %load55 = load i64, ptr %local1, align 4
  store i64 %load55, ptr %local30, align 4
  br label %bb13

bb13:                                             ; preds = %bb12
  store i64 0, ptr %local31, align 4
  br label %bb14

bb14:                                             ; preds = %bb13
  %load56 = load i64, ptr %local30, align 4
  %load57 = load i64, ptr %local31, align 4
  %cmp58 = icmp eq i64 %load56, %load57
  store i1 %cmp58, ptr %local32, align 1
  br label %bb15

bb15:                                             ; preds = %bb14
  %load59 = load i1, ptr %local32, align 1
  br i1 %load59, label %bb45, label %bb16

bb16:                                             ; preds = %bb15
  %load60 = load i64, ptr %local1, align 4
  store i64 %load60, ptr %local27, align 4
  br label %bb17

bb17:                                             ; preds = %bb16
  store i64 1, ptr %local28, align 4
  br label %bb18

bb18:                                             ; preds = %bb17
  %load61 = load i64, ptr %local27, align 4
  %load62 = load i64, ptr %local28, align 4
  %cmp63 = icmp eq i64 %load61, %load62
  store i1 %cmp63, ptr %local29, align 1
  br label %bb19

bb19:                                             ; preds = %bb18
  %load64 = load i1, ptr %local29, align 1
  br i1 %load64, label %bb43, label %bb20

bb20:                                             ; preds = %bb19
  %load65 = load i64, ptr %local1, align 4
  store i64 %load65, ptr %local26, align 4
  br label %bb21

bb21:                                             ; preds = %bb20
  %load66 = load i64, ptr %local26, align 4
  %call = call i1 @is_even(i64 %load66)
  store i1 %call, ptr %local25, align 1
  br label %bb22

bb22:                                             ; preds = %bb21
  %load67 = load i1, ptr %local25, align 1
  br i1 %load67, label %bb31, label %bb23

bb23:                                             ; preds = %bb22
  %load68 = load i64, ptr %local, align 4
  store i64 %load68, ptr %local19, align 4
  br label %bb24

bb24:                                             ; preds = %bb23
  %load69 = load i64, ptr %local, align 4
  store i64 %load69, ptr %local21, align 4
  br label %bb25

bb25:                                             ; preds = %bb24
  %load70 = load i64, ptr %local1, align 4
  store i64 %load70, ptr %local23, align 4
  br label %bb26

bb26:                                             ; preds = %bb25
  store i64 1, ptr %local24, align 4
  br label %bb27

bb27:                                             ; preds = %bb26
  %load71 = load i64, ptr %local23, align 4
  %load72 = load i64, ptr %local24, align 4
  %sub = sub i64 %load71, %load72
  store i64 %sub, ptr %local22, align 4
  br label %bb28

bb28:                                             ; preds = %bb27
  %load73 = load i64, ptr %local21, align 4
  %load74 = load i64, ptr %local22, align 4
  %call75 = call i64 @pow(i64 %load73, i64 %load74)
  store i64 %call75, ptr %local20, align 4
  br label %bb29

bb29:                                             ; preds = %bb28
  %load76 = load i64, ptr %local19, align 4
  %load77 = load i64, ptr %local20, align 4
  %mul = mul i64 %load76, %load77
  store i64 %mul, ptr %local18, align 4
  br label %bb30

bb30:                                             ; preds = %bb29
  %load78 = load i64, ptr %local18, align 4
  ret i64 %load78

bb31:                                             ; preds = %bb22
  %load79 = load i64, ptr %local, align 4
  store i64 %load79, ptr %local14, align 4
  br label %bb32

bb32:                                             ; preds = %bb31
  %load80 = load i64, ptr %local1, align 4
  store i64 %load80, ptr %local16, align 4
  br label %bb33

bb33:                                             ; preds = %bb32
  store i64 2, ptr %local17, align 4
  br label %bb34

bb34:                                             ; preds = %bb33
  %load81 = load i64, ptr %local16, align 4
  %load82 = load i64, ptr %local17, align 4
  %div = sdiv i64 %load81, %load82
  store i64 %div, ptr %local15, align 4
  br label %bb35

bb35:                                             ; preds = %bb34
  %load83 = load i64, ptr %local14, align 4
  %load84 = load i64, ptr %local15, align 4
  %call85 = call i64 @pow(i64 %load83, i64 %load84)
  store i64 %call85, ptr %local8, align 4
  br label %bb36

bb36:                                             ; preds = %bb35
  %load86 = load i64, ptr %local, align 4
  store i64 %load86, ptr %local10, align 4
  br label %bb37

bb37:                                             ; preds = %bb36
  %load87 = load i64, ptr %local1, align 4
  store i64 %load87, ptr %local12, align 4
  br label %bb38

bb38:                                             ; preds = %bb37
  store i64 2, ptr %local13, align 4
  br label %bb39

bb39:                                             ; preds = %bb38
  %load88 = load i64, ptr %local12, align 4
  %load89 = load i64, ptr %local13, align 4
  %div90 = sdiv i64 %load88, %load89
  store i64 %div90, ptr %local11, align 4
  br label %bb40

bb40:                                             ; preds = %bb39
  %load91 = load i64, ptr %local10, align 4
  %load92 = load i64, ptr %local11, align 4
  %call93 = call i64 @pow(i64 %load91, i64 %load92)
  store i64 %call93, ptr %local9, align 4
  br label %bb41

bb41:                                             ; preds = %bb40
  %load94 = load i64, ptr %local8, align 4
  %load95 = load i64, ptr %local9, align 4
  %mul96 = mul i64 %load94, %load95
  store i64 %mul96, ptr %local7, align 4
  br label %bb42

bb42:                                             ; preds = %bb41
  %load97 = load i64, ptr %local7, align 4
  ret i64 %load97

bb43:                                             ; preds = %bb19
  %load98 = load i64, ptr %local, align 4
  store i64 %load98, ptr %local6, align 4
  br label %bb44

bb44:                                             ; preds = %bb43
  %load99 = load i64, ptr %local6, align 4
  ret i64 %load99

bb45:                                             ; preds = %bb15
  store i64 1, ptr %local5, align 4
  br label %bb46

bb46:                                             ; preds = %bb45
  %load100 = load i64, ptr %local5, align 4
  ret i64 %load100

bb47:                                             ; preds = %bb11
  store i64 0, ptr %local4, align 4
  br label %bb48

bb48:                                             ; preds = %bb47
  %load101 = load i64, ptr %local4, align 4
  ret i64 %load101

bb49:                                             ; preds = %bb7
  store i64 1, ptr %local3, align 4
  br label %bb50

bb50:                                             ; preds = %bb49
  %load102 = load i64, ptr %local3, align 4
  ret i64 %load102

bb51:                                             ; preds = %bb3
  store i64 0, ptr %local2, align 4
  br label %bb52

bb52:                                             ; preds = %bb51
  %load103 = load i64, ptr %local2, align 4
  ret i64 %load103
}

define i64 @read_int() {
entry:
  %local = alloca i64, align 8
  br label %bb0

bb0:                                              ; preds = %entry
  %read_int_slot = alloca i64, align 8
  %0 = call i32 (ptr, ...) @scanf(ptr @fmt_read_int, ptr %read_int_slot)
  %read_int_val = load i64, ptr %read_int_slot, align 4
  store i64 %read_int_val, ptr %local, align 4
  br label %bb1

bb1:                                              ; preds = %bb0
  %load = load i64, ptr %local, align 4
  ret i64 %load
}

define void @exit(i64 %0) {
entry:
  %local = alloca i64, align 8
  %local1 = alloca i64, align 8
  store i64 %0, ptr %local, align 4
  br label %bb0

bb0:                                              ; preds = %entry
  %load = load i64, ptr %local, align 4
  store i64 %load, ptr %local1, align 4
  br label %bb1

bb1:                                              ; preds = %bb0
  %load2 = load i64, ptr %local1, align 4
  %exit_code = trunc i64 %load2 to i32
  call void @exit(i32 %exit_code)
  br label %bb2

bb2:                                              ; preds = %bb1
  ret void
}

define i64 @compare(i64 %0, i64 %1) {
entry:
  %local = alloca i64, align 8
  %local1 = alloca i64, align 8
  %local2 = alloca i64, align 8
  %local3 = alloca i64, align 8
  %local4 = alloca i64, align 8
  %local5 = alloca i64, align 8
  %local6 = alloca i64, align 8
  %local7 = alloca i1, align 1
  %local8 = alloca i64, align 8
  %local9 = alloca i64, align 8
  %local10 = alloca i1, align 1
  store i64 %0, ptr %local, align 4
  store i64 %1, ptr %local1, align 4
  br label %bb0

bb0:                                              ; preds = %entry
  %load = load i64, ptr %local, align 4
  store i64 %load, ptr %local8, align 4
  br label %bb1

bb1:                                              ; preds = %bb0
  %load11 = load i64, ptr %local1, align 4
  store i64 %load11, ptr %local9, align 4
  br label %bb2

bb2:                                              ; preds = %bb1
  %load12 = load i64, ptr %local8, align 4
  %load13 = load i64, ptr %local9, align 4
  %cmp = icmp sgt i64 %load12, %load13
  store i1 %cmp, ptr %local10, align 1
  br label %bb3

bb3:                                              ; preds = %bb2
  %load14 = load i1, ptr %local10, align 1
  br i1 %load14, label %bb12, label %bb4

bb4:                                              ; preds = %bb3
  %load15 = load i64, ptr %local, align 4
  store i64 %load15, ptr %local5, align 4
  br label %bb5

bb5:                                              ; preds = %bb4
  %load16 = load i64, ptr %local1, align 4
  store i64 %load16, ptr %local6, align 4
  br label %bb6

bb6:                                              ; preds = %bb5
  %load17 = load i64, ptr %local5, align 4
  %load18 = load i64, ptr %local6, align 4
  %cmp19 = icmp slt i64 %load17, %load18
  store i1 %cmp19, ptr %local7, align 1
  br label %bb7

bb7:                                              ; preds = %bb6
  %load20 = load i1, ptr %local7, align 1
  br i1 %load20, label %bb10, label %bb8

bb8:                                              ; preds = %bb7
  store i64 0, ptr %local4, align 4
  br label %bb9

bb9:                                              ; preds = %bb8
  %load21 = load i64, ptr %local4, align 4
  ret i64 %load21

bb10:                                             ; preds = %bb7
  store i64 2, ptr %local3, align 4
  br label %bb11

bb11:                                             ; preds = %bb10
  %load22 = load i64, ptr %local3, align 4
  ret i64 %load22

bb12:                                             ; preds = %bb3
  store i64 1, ptr %local2, align 4
  br label %bb13

bb13:                                             ; preds = %bb12
  %load23 = load i64, ptr %local2, align 4
  ret i64 %load23
}

define i64 @cmp_2(i64 %0, i64 %1) {
entry:
  %local = alloca i64, align 8
  %local1 = alloca i64, align 8
  %local2 = alloca i64, align 8
  %local3 = alloca i64, align 8
  %local4 = alloca i64, align 8
  %local5 = alloca i64, align 8
  %local6 = alloca i1, align 1
  store i64 %0, ptr %local, align 4
  store i64 %1, ptr %local1, align 4
  br label %bb0

bb0:                                              ; preds = %entry
  %load = load i64, ptr %local, align 4
  store i64 %load, ptr %local4, align 4
  br label %bb1

bb1:                                              ; preds = %bb0
  %load7 = load i64, ptr %local1, align 4
  store i64 %load7, ptr %local5, align 4
  br label %bb2

bb2:                                              ; preds = %bb1
  %load8 = load i64, ptr %local4, align 4
  %load9 = load i64, ptr %local5, align 4
  %cmp = icmp sgt i64 %load8, %load9
  store i1 %cmp, ptr %local6, align 1
  br label %bb3

bb3:                                              ; preds = %bb2
  %load10 = load i1, ptr %local6, align 1
  br i1 %load10, label %bb6, label %bb4

bb4:                                              ; preds = %bb3
  store i64 1, ptr %local3, align 4
  br label %bb5

bb5:                                              ; preds = %bb4
  %load11 = load i64, ptr %local3, align 4
  ret i64 %load11

bb6:                                              ; preds = %bb3
  store i64 0, ptr %local2, align 4
  br label %bb7

bb7:                                              ; preds = %bb6
  %load12 = load i64, ptr %local2, align 4
  ret i64 %load12
}

define void @main() {
entry:
  %local = alloca i64, align 8
  %local1 = alloca i64, align 8
  %local2 = alloca i64, align 8
  %local3 = alloca i64, align 8
  %local4 = alloca i64, align 8
  %local5 = alloca i64, align 8
  %local6 = alloca i1, align 1
  %local7 = alloca i64, align 8
  %local8 = alloca i64, align 8
  %local9 = alloca i64, align 8
  %local10 = alloca i64, align 8
  %local11 = alloca i1, align 1
  %local12 = alloca i64, align 8
  %local13 = alloca i64, align 8
  %local14 = alloca i64, align 8
  %local15 = alloca i64, align 8
  %local16 = alloca i64, align 8
  %local17 = alloca i64, align 8
  %local18 = alloca {}, align 8
  %local19 = alloca i64, align 8
  %local20 = alloca i64, align 8
  %local21 = alloca i64, align 8
  %local22 = alloca i64, align 8
  %local23 = alloca i64, align 8
  %local24 = alloca i64, align 8
  %local25 = alloca i64, align 8
  %local26 = alloca i64, align 8
  %local27 = alloca i64, align 8
  %local28 = alloca i64, align 8
  %local29 = alloca i1, align 1
  %local30 = alloca i1, align 1
  %local31 = alloca i1, align 1
  %local32 = alloca i64, align 8
  %local33 = alloca i64, align 8
  %local34 = alloca i64, align 8
  %local35 = alloca i64, align 8
  %local36 = alloca i64, align 8
  %local37 = alloca i64, align 8
  %local38 = alloca i1, align 1
  br label %bb0

bb0:                                              ; preds = %entry
  store i64 2, ptr %local, align 4
  br label %bb1

bb1:                                              ; preds = %bb0
  store i64 0, ptr %local1, align 4
  br label %bb43

bb2:                                              ; preds = %bb43
  %load = load i64, ptr %local1, align 4
  store i64 %load, ptr %local36, align 4
  br label %bb3

bb3:                                              ; preds = %bb2
  store i64 5, ptr %local37, align 4
  br label %bb4

bb4:                                              ; preds = %bb3
  %load39 = load i64, ptr %local36, align 4
  %load40 = load i64, ptr %local37, align 4
  %cmp = icmp slt i64 %load39, %load40
  store i1 %cmp, ptr %local38, align 1
  br label %bb5

bb5:                                              ; preds = %bb4
  %load41 = load i1, ptr %local38, align 1
  br i1 %load41, label %bb6, label %bb44

bb6:                                              ; preds = %bb5
  %load42 = load i64, ptr %local, align 4
  store i64 %load42, ptr %local34, align 4
  br label %bb7

bb7:                                              ; preds = %bb6
  %load43 = load i64, ptr %local1, align 4
  store i64 %load43, ptr %local35, align 4
  br label %bb8

bb8:                                              ; preds = %bb7
  %load44 = load i64, ptr %local34, align 4
  %load45 = load i64, ptr %local35, align 4
  %call = call i64 @compare(i64 %load44, i64 %load45)
  store i64 %call, ptr %local2, align 4
  br label %bb9

bb9:                                              ; preds = %bb8
  %load46 = load i64, ptr %local, align 4
  store i64 %load46, ptr %local32, align 4
  br label %bb10

bb10:                                             ; preds = %bb9
  %load47 = load i64, ptr %local1, align 4
  store i64 %load47, ptr %local33, align 4
  br label %bb11

bb11:                                             ; preds = %bb10
  %load48 = load i64, ptr %local32, align 4
  %load49 = load i64, ptr %local33, align 4
  %call50 = call i64 @compare(i64 %load48, i64 %load49)
  store i64 %call50, ptr %local3, align 4
  br label %bb12

bb12:                                             ; preds = %bb11
  %load51 = load i64, ptr %local3, align 4
  store i64 %load51, ptr %local19, align 4
  br label %bb13

bb13:                                             ; preds = %bb12
  %load52 = load i64, ptr %local19, align 4
  %cmp53 = icmp eq i64 %load52, 1
  store i1 %cmp53, ptr %local31, align 1
  %load54 = load i1, ptr %local31, align 1
  br i1 %load54, label %bb32, label %bb14

bb14:                                             ; preds = %bb13
  %load55 = load i64, ptr %local19, align 4
  %cmp56 = icmp eq i64 %load55, 2
  store i1 %cmp56, ptr %local30, align 1
  %load57 = load i1, ptr %local30, align 1
  br i1 %load57, label %bb24, label %bb15

bb15:                                             ; preds = %bb14
  %load58 = load i64, ptr %local19, align 4
  %cmp59 = icmp eq i64 %load58, 0
  store i1 %cmp59, ptr %local29, align 1
  %load60 = load i1, ptr %local29, align 1
  br i1 %load60, label %bb17, label %bb16

bb16:                                             ; preds = %bb15
  unreachable

bb17:                                             ; preds = %bb15
  %load61 = load i64, ptr %local, align 4
  store i64 %load61, ptr %local28, align 4
  br label %bb18

bb18:                                             ; preds = %bb17
  %load62 = load i64, ptr %local28, align 4
  %0 = call i32 (ptr, ...) @printf(ptr @fmt_int, i64 %load62)
  br label %bb19

bb19:                                             ; preds = %bb18
  %load63 = load i64, ptr %local2, align 4
  store i64 %load63, ptr %local27, align 4
  br label %bb20

bb20:                                             ; preds = %bb19
  %load64 = load i64, ptr %local27, align 4
  %variant_name_ptr = getelementptr inbounds [3 x ptr], ptr @__enum_2_variants, i64 0, i64 %load64
  %variant_name = load ptr, ptr %variant_name_ptr, align 8
  %1 = call i32 (ptr, ...) @printf(ptr @fmt_enum, ptr %variant_name)
  br label %bb21

bb21:                                             ; preds = %bb20
  %load65 = load i64, ptr %local1, align 4
  store i64 %load65, ptr %local26, align 4
  br label %bb22

bb22:                                             ; preds = %bb21
  %load66 = load i64, ptr %local26, align 4
  %2 = call i32 (ptr, ...) @printf(ptr @fmt_int.1, i64 %load66)
  %3 = call i32 (ptr, ...) @printf(ptr @fmt_nl)
  br label %bb23

bb23:                                             ; preds = %bb22
  store {} zeroinitializer, ptr %local18, align 1
  br label %bb39

bb24:                                             ; preds = %bb14
  %load67 = load i64, ptr %local1, align 4
  store i64 %load67, ptr %local25, align 4
  br label %bb25

bb25:                                             ; preds = %bb24
  %load68 = load i64, ptr %local25, align 4
  %4 = call i32 (ptr, ...) @printf(ptr @fmt_int.2, i64 %load68)
  br label %bb26

bb26:                                             ; preds = %bb25
  store i64 1, ptr %local2, align 4
  br label %bb27

bb27:                                             ; preds = %bb26
  %load69 = load i64, ptr %local2, align 4
  store i64 %load69, ptr %local24, align 4
  br label %bb28

bb28:                                             ; preds = %bb27
  %load70 = load i64, ptr %local24, align 4
  %variant_name_ptr71 = getelementptr inbounds [3 x ptr], ptr @__enum_2_variants, i64 0, i64 %load70
  %variant_name72 = load ptr, ptr %variant_name_ptr71, align 8
  %5 = call i32 (ptr, ...) @printf(ptr @fmt_enum.3, ptr %variant_name72)
  br label %bb29

bb29:                                             ; preds = %bb28
  %load73 = load i64, ptr %local, align 4
  store i64 %load73, ptr %local23, align 4
  br label %bb30

bb30:                                             ; preds = %bb29
  %load74 = load i64, ptr %local23, align 4
  %6 = call i32 (ptr, ...) @printf(ptr @fmt_int.4, i64 %load74)
  %7 = call i32 (ptr, ...) @printf(ptr @fmt_nl.5)
  br label %bb31

bb31:                                             ; preds = %bb30
  store {} zeroinitializer, ptr %local18, align 1
  br label %bb39

bb32:                                             ; preds = %bb13
  %load75 = load i64, ptr %local, align 4
  store i64 %load75, ptr %local22, align 4
  br label %bb33

bb33:                                             ; preds = %bb32
  %load76 = load i64, ptr %local22, align 4
  %8 = call i32 (ptr, ...) @printf(ptr @fmt_int.6, i64 %load76)
  br label %bb34

bb34:                                             ; preds = %bb33
  %load77 = load i64, ptr %local2, align 4
  store i64 %load77, ptr %local21, align 4
  br label %bb35

bb35:                                             ; preds = %bb34
  %load78 = load i64, ptr %local21, align 4
  %variant_name_ptr79 = getelementptr inbounds [3 x ptr], ptr @__enum_2_variants, i64 0, i64 %load78
  %variant_name80 = load ptr, ptr %variant_name_ptr79, align 8
  %9 = call i32 (ptr, ...) @printf(ptr @fmt_enum.7, ptr %variant_name80)
  br label %bb36

bb36:                                             ; preds = %bb35
  %load81 = load i64, ptr %local1, align 4
  store i64 %load81, ptr %local20, align 4
  br label %bb37

bb37:                                             ; preds = %bb36
  %load82 = load i64, ptr %local20, align 4
  %10 = call i32 (ptr, ...) @printf(ptr @fmt_int.8, i64 %load82)
  %11 = call i32 (ptr, ...) @printf(ptr @fmt_nl.9)
  br label %bb38

bb38:                                             ; preds = %bb37
  store {} zeroinitializer, ptr %local18, align 1
  br label %bb39

bb39:                                             ; preds = %bb38, %bb31, %bb23
  %load83 = load i64, ptr %local1, align 4
  store i64 %load83, ptr %local16, align 4
  br label %bb40

bb40:                                             ; preds = %bb39
  store i64 1, ptr %local17, align 4
  br label %bb41

bb41:                                             ; preds = %bb40
  %load84 = load i64, ptr %local16, align 4
  %load85 = load i64, ptr %local17, align 4
  %add = add i64 %load84, %load85
  store i64 %add, ptr %local1, align 4
  br label %bb42

bb42:                                             ; preds = %bb41
  br label %bb43

bb43:                                             ; preds = %bb42, %bb1
  br label %bb2

bb44:                                             ; preds = %bb5
  store i64 0, ptr %local4, align 4
  br label %bb45

bb45:                                             ; preds = %bb44
  %load86 = load i64, ptr %local4, align 4
  store i64 %load86, ptr %local5, align 4
  br label %bb46

bb46:                                             ; preds = %bb45
  store i64 1, ptr %local4, align 4
  br label %bb47

bb47:                                             ; preds = %bb46
  %load87 = load i64, ptr %local4, align 4
  store i64 %load87, ptr %local15, align 4
  br label %bb48

bb48:                                             ; preds = %bb47
  %load88 = load i64, ptr %local15, align 4
  %variant_name_ptr89 = getelementptr inbounds [3 x ptr], ptr @__enum_0_variants, i64 0, i64 %load88
  %variant_name90 = load ptr, ptr %variant_name_ptr89, align 8
  %12 = call i32 (ptr, ...) @printf(ptr @fmt_enum.10, ptr %variant_name90)
  %13 = call i32 (ptr, ...) @printf(ptr @fmt_nl.11)
  br label %bb49

bb49:                                             ; preds = %bb48
  %load91 = load i64, ptr %local5, align 4
  store i64 %load91, ptr %local14, align 4
  br label %bb50

bb50:                                             ; preds = %bb49
  %load92 = load i64, ptr %local14, align 4
  %variant_name_ptr93 = getelementptr inbounds [3 x ptr], ptr @__enum_0_variants, i64 0, i64 %load92
  %variant_name94 = load ptr, ptr %variant_name_ptr93, align 8
  %14 = call i32 (ptr, ...) @printf(ptr @fmt_enum.12, ptr %variant_name94)
  %15 = call i32 (ptr, ...) @printf(ptr @fmt_nl.13)
  br label %bb51

bb51:                                             ; preds = %bb50
  store i64 1, ptr %local12, align 4
  br label %bb52

bb52:                                             ; preds = %bb51
  store i64 2, ptr %local13, align 4
  br label %bb53

bb53:                                             ; preds = %bb52
  %load95 = load i64, ptr %local12, align 4
  %load96 = load i64, ptr %local13, align 4
  %cmp97 = icmp eq i64 %load95, %load96
  store i1 %cmp97, ptr %local11, align 1
  br label %bb54

bb54:                                             ; preds = %bb53
  %load98 = load i1, ptr %local11, align 1
  %bool_ext = zext i1 %load98 to i32
  %16 = call i32 (ptr, ...) @printf(ptr @fmt_bool, i32 %bool_ext)
  %17 = call i32 (ptr, ...) @printf(ptr @fmt_nl.14)
  br label %bb55

bb55:                                             ; preds = %bb54
  store i64 2, ptr %local9, align 4
  br label %bb56

bb56:                                             ; preds = %bb55
  store i64 2, ptr %local10, align 4
  br label %bb57

bb57:                                             ; preds = %bb56
  %load99 = load i64, ptr %local9, align 4
  %load100 = load i64, ptr %local10, align 4
  %call101 = call i64 @compare(i64 %load99, i64 %load100)
  store i64 %call101, ptr %local7, align 4
  br label %bb58

bb58:                                             ; preds = %bb57
  store i64 0, ptr %local8, align 4
  br label %bb59

bb59:                                             ; preds = %bb58
  %load102 = load i64, ptr %local7, align 4
  %load103 = load i64, ptr %local8, align 4
  %cmp104 = icmp eq i64 %load102, %load103
  store i1 %cmp104, ptr %local6, align 1
  br label %bb60

bb60:                                             ; preds = %bb59
  %load105 = load i1, ptr %local6, align 1
  %bool_ext106 = zext i1 %load105 to i32
  %18 = call i32 (ptr, ...) @printf(ptr @fmt_bool.15, i32 %bool_ext106)
  %19 = call i32 (ptr, ...) @printf(ptr @fmt_nl.16)
  br label %bb61

bb61:                                             ; preds = %bb60
  ret void
}

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i64 @llvm.abs.i64(i64, i1 immarg) #0

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i64 @llvm.smin.i64(i64, i64) #0

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i64 @llvm.smax.i64(i64, i64) #0

declare i32 @scanf(ptr, ...)

declare i32 @printf(ptr, ...)

attributes #0 = { nocallback nofree nosync nounwind speculatable willreturn memory(none) }
