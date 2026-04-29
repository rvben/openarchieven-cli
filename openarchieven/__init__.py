"""
openarchieven: CLI for the Open Archives Dutch genealogical API.
"""

try:
    from importlib.metadata import version
    __version__ = version("openarchieven")
except ImportError:
    from importlib_metadata import version
    __version__ = version("openarchieven")
