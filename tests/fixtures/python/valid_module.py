import os
from typing import List


def load(path):
    return os.path.exists(path)


class Store:
    def put(self, key, value):
        self._data[key] = value
