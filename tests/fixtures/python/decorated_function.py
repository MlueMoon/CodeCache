@cache
@retry(3)
def compute(value):
    return value * 2
