"""
CV3 State Management
Handles variables, state storage, and state change events for CV3 graphs.
"""

from enum import Enum
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Any, Callable
import uuid
import json

from .graph import VariableType


class CV3Variable:
    """A variable that can store state in a CV3 graph."""
    
    def __init__(self, name: str, var_type: 'VariableType', initial_value: Any = None):
        self.id = str(uuid.uuid4())
        self.name = name
        self.var_type = var_type
        self.value = initial_value
        self._subscribers: List[Callable[[str, Any], None]] = []
        self._change_history: List[tuple[float, Any]] = []  # (timestamp, value)
        self._record_change(initial_value)
    
    def _record_change(self, value: Any):
        """Record a value change for history and debugging."""
        import time
        self._change_history.append((time.time(), value))
        # Keep only last 100 changes to prevent memory growth
        if len(self._change_history) > 100:
            self._change_history = self._change_history[-100:]
        
        # Notify subscribers
        for subscriber in self._subscribers:
            try:
                subscriber(self.name, value)
            except Exception:
                pass  # Don't let subscriber errors break the system
    
    def set_value(self, value: Any) -> bool:
        """Set the variable's value with type checking."""
        # Type checking would be more comprehensive in a full implementation
        if self.var_type == VariableType.BOOL and not isinstance(value, bool):
            return False
        elif self.var_type == VariableType.INT and not isinstance(value, int):
            return False
        elif self.var_type == VariableType.FLOAT and not isinstance(value, (int, float)):
            return False
        elif self.var_type == VariableType.STRING and not isinstance(value, str):
            return False
        # For simplicity, we'll allow the assignment and trust the graph validation
        
        old_value = self.value
        self.value = value
        self._record_change(value)
        return True
    
    def get_value(self) -> Any:
        """Get the current value of the variable."""
        return self.value
    
    def subscribe(self, callback: Callable[[str, Any], None]):
        """Subscribe to value change events."""
        self._subscribers.append(callback)
    
    def unsubscribe(self, callback: Callable[[str, Any], None]):
        """Unsubscribe from value change events."""
        if callback in self._subscribers:
            self._subscribers.remove(callback)
    
    def get_change_history(self) -> List[tuple[float, Any]]:
        """Get the history of value changes."""
        return self._change_history.copy()


class CV3State:
    """Manages the state of a CV3 graph execution."""
    
    def __init__(self):
        self.variables: Dict[str, CV3Variable] = {}
        self.execution_stack: List[str] = []  # Stack of currently executing node IDs
        self.tick_count = 0
        self.start_time = 0.0
    
    def create_variable(self, name: str, var_type: 'VariableType', initial_value: Any = None) -> CV3Variable:
        """Create a new variable and store it in the state."""
        var = CV3Variable(name, var_type, initial_value)
        self.variables[var.id] = var
        return var
    
    def get_variable(self, var_id: str) -> Optional[CV3Variable]:
        """Get a variable by its ID."""
        return self.variables.get(var_id)
    
    def get_variable_by_name(self, name: str) -> Optional[CV3Variable]:
        """Get a variable by its name."""
        for var in self.variables.values():
            if var.name == name:
                return var
        return None
    
    def delete_variable(self, var_id: str) -> bool:
        """Delete a variable from the state."""
        if var_id in self.variables:
            del self.variables[var_id]
            return True
        return False
    
    def push_execution(self, node_id: str):
        """Push a node onto the execution stack."""
        self.execution_stack.append(node_id)
    
    def pop_execution(self) -> Optional[str]:
        """Pop a node from the execution stack."""
        if self.execution_stack:
            return self.execution_stack.pop()
        return None
    
    def get_execution_stack(self) -> List[str]:
        """Get a copy of the current execution stack."""
        return self.execution_stack.copy()
    
    def reset_tick_count(self):
        """Reset the tick counter."""
        self.tick_count = 0
    
    def increment_tick(self):
        """Increment the tick counter."""
        self.tick_count += 1
    
    def set_start_time(self, time: float):
        """Set the start time for execution timing."""
        self.start_time = time
    
    def get_elapsed_time(self) -> float:
        """Get elapsed time since start."""
        import time
        return time.time() - self.start_time
    
    def serialize(self) -> Dict[str, Any]:
        """Serialize the state for saving."""
        return {
            "variables": {
                var_id: {
                    "name": var.name,
                    "type": var.var_type.value,
                    "value": var.get_value(),
                    "change_history": var.get_change_history()
                }
                for var_id, var in self.variables.items()
            },
            "execution_stack": self.execution_stack.copy(),
            "tick_count": self.tick_count,
            "start_time": self.start_time
        }
    
    def deserialize(self, data: Dict[str, Any]):
        """Deserialize state from saved data."""
        from .graph import VariableType
        
        self.variables = {}
        for var_id, var_data in data.get("variables", {}).items():
            var_type = VariableType(var_data["type"])
            var = CV3Variable(var_data["name"], var_type, var_data["value"])
            # Note: change history is not restored for simplicity
            self.variables[var_id] = var
        
        self.execution_stack = data.get("execution_stack", []).copy()
        self.tick_count = data.get("tick_count", 0)
        self.start_time = data.get("start_time", 0.0)