"""
CV3 Tracer
Provides debug tracing capabilities for CV3 graph execution.
"""

from enum import Enum
from dataclasses import dataclass, field
from typing import List, Optional, Any, Callable
import time
import uuid


class TraceEventType(Enum):
    """Types of events that can be traced."""
    NODE_START = "node_start"
    NODE_END = "node_end"
    VARIABLE_SET = "variable_set"
    VARIABLE_GET = "variable_get"
    QUOTA_VIOLATION = "quota_violation"
    GRAPH_VALIDATION = "graph_validation"
    EXECUTION_START = "execution_start"
    EXECUTION_END = "execution_end"


@dataclass
class TraceEvent:
    """A single trace event."""
    id: str
    event_type: TraceEventType
    timestamp: float
    node_id: Optional[str] = None
    node_name: Optional[str] = None
    variable_name: Optional[str] = None
    variable_value: Any = None
    quota_type: Optional[str] = None
    quota_usage: Optional[float] = None
    quota_limit: Optional[float] = None
    message: Optional[str] = None
    data: dict = field(default_factory=dict)
    
    def __post_init__(self):
        if not self.id:
            self.id = str(uuid.uuid4())
        if self.timestamp == 0:
            self.timestamp = time.time()


class CV3Tracer:
    """Traces CV3 graph execution for debugging and profiling."""
    
    def __init__(self, max_events: int = 10000):
        self.events: List[TraceEvent] = []
        self.max_events = max_events
        self.enabled = False
        self._event_callbacks: List[Callable[[TraceEvent], None]] = []
    
    def enable(self):
        """Enable tracing."""
        self.enabled = True
        self.clear()
    
    def disable(self):
        """Disable tracing."""
        self.enabled = False
    
    def clear(self):
        """Clear all trace events."""
        self.events.clear()
    
    def add_callback(self, callback: Callable[[TraceEvent], None]):
        """Add a callback to be called when events are traced."""
        self._event_callbacks.append(callback)
    
    def remove_callback(self, callback: Callable[[TraceEvent], None]):
        """Remove a callback."""
        if callback in self._event_callbacks:
            self._event_callbacks.remove(callback)
    
    def _trace_event(self, event: TraceEvent):
        """Internal method to trace an event."""
        if not self.enabled:
            return
        
        self.events.append(event)
        
        # Keep only the most recent events to prevent memory growth
        if len(self.events) > self.max_events:
            self.events = self.events[-self.max_events:]
        
        # Call callbacks
        for callback in self._event_callbacks:
            try:
                callback(event)
            except Exception:
                pass  # Don't let callback errors break tracing
    
    def trace_node_start(self, node_id: str, node_name: str):
        """Trace the start of node execution."""
        event = TraceEvent(
            id="",
            event_type=TraceEventType.NODE_START,
            timestamp=time.time(),
            node_id=node_id,
            node_name=node_name
        )
        self._trace_event(event)
    
    def trace_node_end(self, node_id: str, node_name: str, success: bool = True):
        """Trace the end of node execution."""
        event = TraceEvent(
            id="",
            event_type=TraceEventType.NODE_END,
            timestamp=time.time(),
            node_id=node_id,
            node_name=node_name,
            data={"success": success}
        )
        self._trace_event(event)
    
    def trace_variable_set(self, variable_name: str, value: Any):
        """Trace setting a variable."""
        event = TraceEvent(
            id="",
            event_type=TraceEventType.VARIABLE_SET,
            timestamp=time.time(),
            variable_name=variable_name,
            variable_value=value
        )
        self._trace_event(event)
    
    def trace_variable_get(self, variable_name: str, value: Any):
        """Trace getting a variable."""
        event = TraceEvent(
            id="",
            event_type=TraceEventType.VARIABLE_GET,
            timestamp=time.time(),
            variable_name=variable_name,
            variable_value=value
        )
        self._trace_event(event)
    
    def trace_quota_violation(self, quota_type: str, usage: float, limit: float):
        """Trace a quota violation."""
        event = TraceEvent(
            id="",
            event_type=TraceEventType.QUOTA_VIOLATION,
            timestamp=time.time(),
            quota_type=quota_type,
            quota_usage=usage,
            quota_limit=limit
        )
        self._trace_event(event)
    
    def trace_graph_validation(self, is_valid: bool, errors: List[str]):
        """Trace graph validation results."""
        event = TraceEvent(
            id="",
            event_type=TraceEventType.GRAPH_VALIDATION,
            timestamp=time.time(),
            message=f"Graph validation: {'PASS' if is_valid else 'FAIL'}",
            data={"is_valid": is_valid, "errors": errors}
        )
        self._trace_event(event)
    
    def trace_execution_start(self):
        """Trace the start of graph execution."""
        event = TraceEvent(
            id="",
            event_type=TraceEventType.EXECUTION_START,
            timestamp=time.time(),
            message="Graph execution started"
        )
        self._trace_event(event)
    
    def trace_execution_end(self, success: bool = True):
        """Trace the end of graph execution."""
        event = TraceEvent(
            id="",
            event_type=TraceEventType.EXECUTION_END,
            timestamp=time.time(),
            message=f"Graph execution ended: {'SUCCESS' if success else 'FAILED'}",
            data={"success": success}
        )
        self._trace_event(event)
    
    def get_events(self, event_type: Optional[TraceEventType] = None) -> List[TraceEvent]:
        """Get trace events, optionally filtered by type."""
        if event_type is None:
            return self.events.copy()
        return [event for event in self.events if event.event_type == event_type]
    
    def get_events_for_node(self, node_id: str) -> List[TraceEvent]:
        """Get all trace events for a specific node."""
        return [event for event in self.events if event.node_id == node_id]
    
    def get_events_for_variable(self, variable_name: str) -> List[TraceEvent]:
        """Get all trace events for a specific variable."""
        return [event for event in self.events if event.variable_name == variable_name]
    
    def get_execution_timeline(self) -> List[Dict[str, Any]]:
        """Get a timeline of execution events for visualization."""
        timeline = []
        for event in self.events:
            timeline.append({
                "id": event.id,
                "type": event.event_type.value,
                "time": event.timestamp,
                "node_id": event.node_id,
                "node_name": event.node_name,
                "variable_name": event.variable_name,
                "message": event.message or "",
                "data": event.data
            })
        return timeline
    
    def serialize(self) -> Dict[str, Any]:
        """Serialize trace events for saving."""
        return {
            "enabled": self.enabled,
            "events": [
                {
                    "id": event.id,
                    "event_type": event.event_type.value,
                    "timestamp": event.timestamp,
                    "node_id": event.node_id,
                    "node_name": event.node_name,
                    "variable_name": event.variable_name,
                    "variable_value": event.variable_value,
                    "quota_type": event.quota_type,
                    "quota_usage": event.quota_usage,
                    "quota_limit": event.quota_limit,
                    "message": event.message,
                    "data": event.data
                }
                for event in self.events
            ]
        }
    
    def deserialize(self, data: Dict[str, Any]):
        """Deserialize trace events from saved data."""
        self.enabled = data.get("enabled", False)
        self.events = []
        for event_data in data.get("events", []):
            event = TraceEvent(
                id=event_data["id"],
                event_type=TraceEventType(event_data["event_type"]),
                timestamp=event_data["timestamp"],
                node_id=event_data.get("node_id"),
                node_name=event_data.get("node_name"),
                variable_name=event_data.get("variable_name"),
                variable_value=event_data.get("variable_value"),
                quota_type=event_data.get("quota_type"),
                quota_usage=event_data.get("quota_usage"),
                quota_limit=event_data.get("quota_limit"),
                message=event_data.get("message"),
                data=event_data.get("data", {})
            )
            self.events.append(event)