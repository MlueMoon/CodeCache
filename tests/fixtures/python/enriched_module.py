"""Module docstring: user service helpers."""
import os
from typing import List


def hash_password(raw):
    return os.urandom(16)


class UserService:
    def register(self, name):
        token = hash_password(name)
        return token
