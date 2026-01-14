var _flow = "3|1|0|2|4".split("|");
var _i = 0;
while (true) {
    switch (_flow[_i++]) {
        case "0":
            console.log("step 3");
            continue;
        case "1":
            console.log("step 2");
            continue;
        case "2":
            console.log("step 4");
            continue;
        case "3":
            console.log("step 1");
            continue;
        case "4":
            console.log("step 5");
            break;
    }
    break;
}
