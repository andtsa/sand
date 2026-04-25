; ModuleID = 'sand_module'
source_filename = "sand_module"

@fmt_int = private unnamed_addr constant [5 x i8] c"%ld \00", align 1
@fmt_nl = private unnamed_addr constant [2 x i8] c"\0A\00", align 1

define i64 @fib(i64 %0) {
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
  %local13 = alloca i1, align 1
  %local14 = alloca i64, align 8
  %local15 = alloca i64, align 8
  %local16 = alloca i64, align 8
  store i64 %0, ptr %local, align 4
  br label %bb0

bb0:                                              ; preds = %entry
  store i64 1, ptr %local15, align 4
  br label %bb1

bb1:                                              ; preds = %bb0
  store i64 2, ptr %local16, align 4
  br label %bb2

bb2:                                              ; preds = %bb1
  %load = load i64, ptr %local15, align 4
  %load17 = load i64, ptr %local16, align 4
  %add = add i64 %load, %load17
  store i64 %add, ptr %local14, align 4
  br label %bb3

bb3:                                              ; preds = %bb2
  %load18 = load i64, ptr %local, align 4
  store i64 %load18, ptr %local11, align 4
  br label %bb4

bb4:                                              ; preds = %bb3
  store i64 1, ptr %local12, align 4
  br label %bb5

bb5:                                              ; preds = %bb4
  %load19 = load i64, ptr %local11, align 4
  %load20 = load i64, ptr %local12, align 4
  %cmp = icmp sle i64 %load19, %load20
  store i1 %cmp, ptr %local13, align 1
  br label %bb6

bb6:                                              ; preds = %bb5
  %load21 = load i1, ptr %local13, align 1
  br i1 %load21, label %bb17, label %bb7

bb7:                                              ; preds = %bb6
  %load22 = load i64, ptr %local, align 4
  store i64 %load22, ptr %local9, align 4
  br label %bb8

bb8:                                              ; preds = %bb7
  store i64 1, ptr %local10, align 4
  br label %bb9

bb9:                                              ; preds = %bb8
  %load23 = load i64, ptr %local9, align 4
  %load24 = load i64, ptr %local10, align 4
  %sub = sub i64 %load23, %load24
  store i64 %sub, ptr %local8, align 4
  br label %bb10

bb10:                                             ; preds = %bb9
  %load25 = load i64, ptr %local8, align 4
  %call = call i64 @fib(i64 %load25)
  store i64 %call, ptr %local3, align 4
  br label %bb11

bb11:                                             ; preds = %bb10
  %load26 = load i64, ptr %local, align 4
  store i64 %load26, ptr %local6, align 4
  br label %bb12

bb12:                                             ; preds = %bb11
  store i64 2, ptr %local7, align 4
  br label %bb13

bb13:                                             ; preds = %bb12
  %load27 = load i64, ptr %local6, align 4
  %load28 = load i64, ptr %local7, align 4
  %sub29 = sub i64 %load27, %load28
  store i64 %sub29, ptr %local5, align 4
  br label %bb14

bb14:                                             ; preds = %bb13
  %load30 = load i64, ptr %local5, align 4
  %call31 = call i64 @fib(i64 %load30)
  store i64 %call31, ptr %local4, align 4
  br label %bb15

bb15:                                             ; preds = %bb14
  %load32 = load i64, ptr %local3, align 4
  %load33 = load i64, ptr %local4, align 4
  %add34 = add i64 %load32, %load33
  store i64 %add34, ptr %local2, align 4
  br label %bb16

bb16:                                             ; preds = %bb15
  %load35 = load i64, ptr %local2, align 4
  ret i64 %load35

bb17:                                             ; preds = %bb6
  %load36 = load i64, ptr %local, align 4
  store i64 %load36, ptr %local1, align 4
  br label %bb18

bb18:                                             ; preds = %bb17
  %load37 = load i64, ptr %local1, align 4
  ret i64 %load37
}

define i64 @main() {
entry:
  %local = alloca i64, align 8
  %local1 = alloca i64, align 8
  %local2 = alloca i64, align 8
  %local3 = alloca i64, align 8
  %local4 = alloca i64, align 8
  %local5 = alloca i64, align 8
  br label %bb0

bb0:                                              ; preds = %entry
  store i64 11, ptr %local, align 4
  br label %bb1

bb1:                                              ; preds = %bb0
  %load = load i64, ptr %local, align 4
  store i64 %load, ptr %local5, align 4
  br label %bb2

bb2:                                              ; preds = %bb1
  %load6 = load i64, ptr %local5, align 4
  %call = call i64 @fib(i64 %load6)
  store i64 %call, ptr %local1, align 4
  br label %bb3

bb3:                                              ; preds = %bb2
  %load7 = load i64, ptr %local, align 4
  store i64 %load7, ptr %local4, align 4
  br label %bb4

bb4:                                              ; preds = %bb3
  %load8 = load i64, ptr %local4, align 4
  %call9 = call i64 @fib(i64 %load8)
  store i64 %call9, ptr %local3, align 4
  br label %bb5

bb5:                                              ; preds = %bb4
  %load10 = load i64, ptr %local3, align 4
  %0 = call i32 (ptr, ...) @printf(ptr @fmt_int, i64 %load10)
  %1 = call i32 (ptr, ...) @printf(ptr @fmt_nl)
  br label %bb6

bb6:                                              ; preds = %bb5
  store i64 0, ptr %local2, align 4
  br label %bb7

bb7:                                              ; preds = %bb6
  %load11 = load i64, ptr %local2, align 4
  ret i64 %load11
}

declare i32 @printf(ptr, ...)
