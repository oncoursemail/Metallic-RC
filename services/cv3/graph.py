"""
CV3 Graph Structure
Defines the data structures for Circuits V3 graphs including nodes, pins, and connections.
"""

from enum import Enum
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Any
import uuid


class PinType(Enum):
    """Types of pins in CV3 graphs."""
    EVENT = "event"
    VALUE = "value" 
    EXECUTION = "execution"


class VariableType(Enum):
    """Types of variables supported in CV3."""
    BOOL = "bool"
    INT = "int"
    FLOAT = "float"
    STRING = "string"
    VECTOR3 = "vector3"
    OBJECT_REF = "object_ref"


@dataclass
class CV3Pin:
    """A pin on a CV3 node for connections."""
    id: str
    name: str
    pin_type: PinType
    variable_type: Optional[VariableType] = None
    is_connected: bool = False
    connected_to: Optional[str] = None  # ID of connected pin
    
    def __post_init__(self):
        if not self.id:
            self.id = str(uuid.uuid4())


@dataclass
class CV3Node:
    """A node in a CV3 graph."""
    id: str
    name: str
    node_type: str  # e.g., "Timer", "VariableSet", "MathAdd"
    inputs: List[CV3Pin] = field(default_factory=list)
    outputs: List[CV3Pin] = field(default_factory=list)
    properties: Dict[str, Any] = field(default_factory=dict)
    position: tuple[float, float] = (0.0, 0.0)  # For editor positioning
    
    def __post_init__(self):
        if not self.id:
            self.id = str(uuid.uuid4())
        
        # Initialize default pins based on node type
        if not self.inputs and not self.outputs:
            self._initialize_default_pins()
    
    def _initialize_default_pins(self):
        """Initialize default pins based on node type."""
        if self.node_type == "Timer":
            self.inputs = [
                CV3Pin("", "Start", PinType.EXECUTION),
                CV3Pin("", "Duration", PinType.VALUE, VariableType.FLOAT)
            ]
            self.outputs = [
                CV3Pin("", "Finished", PinType.EXECUTION),
                CV3Pin("", "Progress", PinType.VALUE, VariableType.FLOAT)
            ]
        elif self.node_type == "VariableSet":
            self.inputs = [
                CV3Pin("", "Value", PinType.VALUE),
                CV3Pin("", "Set", PinType.EXECUTION)
            ]
            self.outputs = [
                CV3Pin("", "Out", PinType.EXECUTION)
            ]
        elif self.node_type == "VariableGet":
            self.inputs = [
                CV3Pin("", "Get", PinType.EXECUTION)
            ]
            self.outputs = [
                CV3Pin("", "Value", PinType.VALUE),
                CV3Pin("", "Out", PinType.EXECUTION)
            ]
        elif self.node_type == "MathAdd":
            self.inputs = [
                CV3Pin("", "A", PinType.VALUE, VariableType.FLOAT),
                CV3Pin("", "B", PinType.VALUE, VariableType.FLOAT),
                CV3Pin("", "Add", PinType.EXECUTION)
            ]
            self.outputs = [
                CV3Pin("", "Result", PinType.VALUE, VariableType.FLOAT),
                CV3Pin("", "Out", PinType.EXECUTION)
            ]
        else:
            # Generic node with one execution input and output
            self.inputs = [CV3Pin("", "In", PinType.EXECUTION)]
            self.outputs = [CV3Pin("", "Out", PinType.EXECUTION)]


@dataclass
class CV3Graph:
    """A complete CV3 graph containing nodes and their connections."""
    id: str
    name: str
    nodes: Dict[str, CV3Node] = field(default_factory=dict)
    connections: List[tuple[str, str]] = field(default_factory=list)  # (output_pin_id, input_pin_id)
    metadata: Dict[str, Any] = field(default_factory=dict)
    
    def __post_init__(self):
        if not self.id:
            self.id = str(uuid.uuid4())
    
    def add_node(self, node: CV3Node) -> str:
        """Add a node to the graph and return its ID."""
        self.nodes[node.id] = node
        return node.id
    
    def remove_node(self, node_id: str) -> bool:
        """Remove a node from the graph."""
        if node_id in self.nodes:
            # Remove all connections involving this node
            self.connections = [
                conn for conn in self.connections
                if conn[0] not in self.nodes[node_id].inputs + self.nodes[node_id].outputs
                and conn[1] not in self.nodes[node_id].inputs + self.nodes[node_id].outputs
            ]
            del self.nodes[node_id]
            return True
        return False
    
    def connect_pins(self, output_pin_id: str, input_pin_id: str) -> bool:
        """Connect two pins together."""
        # Find the pins
        output_pin = None
        input_pin = None
        output_node_id = None
        input_node_id = None
        
        for node_id, node in self.nodes.items():
            for pin in node.inputs:
                if pin.id == input_pin_id:
                    input_pin = pin
                    input_node_id = node_id
            for pin in node.outputs:
                if pin.id == output_pin_id:
                    output_pin = pin
                    output_node_id = node_id
        
        if not output_pin or not input_pin:
            return False
            
        # Check compatibility
        if output_pin.pin_type != input_pin.pin_type:
            return False
            
        if output_pin.pin_type == PinType.VALUE and input_pin.pin_type == PinType.VALUE:
            if output_pin.variable_type != input_pin.variable_type:
                return False
        
        # Make the connection
        output_pin.is_connected = True
        output_pin.connected_to = input_pin_id
        input_pin.is_connected = True
        input_pin.connected_to = output_pin_id
        
        self.connections.append((output_pin_id, input_pin_id))
        return True
    
    def disconnect_pins(self, output_pin_id: str, input_pin_id: str) -> bool:
        """Disconnect two pins."""
        if (output_pin_id, input_pin_id) in self.connections:
            self.connections.remove((output_pin_id, input_pin_id))
            
            # Find the pins and update their state
            for node_id, node in self.nodes.items():
                for pin in node.inputs:
                    if pin.id == input_pin_id:
                        pin.is_connected = False
                        pin.connected_to = None
                for pin in node.outputs:
                    if pin.id == output_pin_id:
                        pin.is_connected = False
                        pin.connected_to = None
            return True
        return False
    
    def validate(self) -> List[str]:
        """Validate the graph and return list of errors."""
        errors = []
        
        # Check for disconnected required pins
        for node_id, node in self.nodes.items():
            for pin in node.inputs:
                if pin.pin_type == PinType.EXECUTION and not pin.is_connected:
                    errors.append(f"Node '{node.name}' ({node_id}) has unconnected execution input: {pin.name}")
                elif pin.pin_type == PinType.VALUE and not pin.is_connected:
                    errors.append(f"Node '{node.name}' ({node_id}) has unconnected value input: {pin.name}")
        
        # Check for cycles (simplified)
        # In a full implementation, we'd do proper cycle detection
        
        return errors