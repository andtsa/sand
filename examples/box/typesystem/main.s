def compare(x: Int, y: Int): #gt | #lt | #eq := {
    if x > y then {
        #gt
    } else if x < y then {
        #lt
    } else {
        #eq
    }
}

def cmp_2(x: Int, y: Int): #gt | #lt := {
    if x > y then #gt else #lt
}

type Test = One | Two | Three;

type Result = Ok(Int) | Error(Int);

def main(): Unit := {
    let x = 2;
    
    let mut y = 0;
    while y < 5 do {
        let mut result = (compare(x, y));
        let cmp = compare(x, y);
        match cmp {
            #gt => {
                print(x);
                print(result);
                println(y);
            },
            #lt => {
                print(y);
                result = #gt;
                print(result);
                println(x);
            },
            #eq => {
                print(x);
                print(result);
                println(y);
            }
        };
        y = y + 1;
    };

    let mut z = Test#One;

    let y = z;

    z = Test#Two;

    println(z);
    println(y);
    println(Test#Two == Test#Three);

    println(compare(2, 2) == #eq);
}
