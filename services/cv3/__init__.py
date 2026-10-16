# CV3 Runtime Implementation
# Circuits V3 deterministic runtime for Metallic Studios

"""
CV3 (Circuits V3) Runtime Implementation

This module provides a deterministic runtime for executing Circuits V3 graphs
with features including:
- Typed pins (event, value, execution)
- State management with variables
- Graph validation
- Per-tick execution quotas
- Debug tracing
- Save/rollback functionality
"""

from .runtime import CV3Runtime
from .graph import CV3Graph, CV3Node, CV3Pin, PinType
from .state import CV3State, CV3Variable
from .quotas import CV3Quotas, QuotaType
from .tracer import CV3Tracer, TraceEvent
from .serializer import CV3Serializer

__all__ = [
    "CV3Runtime",
    "CV3Graph", 
    "CV3Node",
    "CV3Pin",
    "PinType",
    "CV3State",
    "CV3Variable",
    "CV3Quotas",
    "QuotaType",
    "CV3Tracer",
    "TraceEvent",
    "CV3Serializer"
]