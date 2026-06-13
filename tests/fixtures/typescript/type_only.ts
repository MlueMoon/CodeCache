interface Shape {
  area(): number;
}

type Pair<T> = { first: T; second: T };

function makePair<T>(a: T, b: T): Pair<T> {
  return { first: a, second: b };
}

class Circle implements Shape {
  area(): number {
    return 3.14;
  }
}
