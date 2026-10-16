"""
CV3 Serializer
Handles saving and loading of CV3 graphs, state, and execution history.
"""

import json
from typing import Dict, Any, Optional
from datetime import datetime


class CV3Serializer:
    """Serializes and deserializes CV3 graphs and associated data."""
    
    @staticmethod
    def serialize_graph(graph) -> Dict[str, Any]:
        """Serialize a CV3 graph to a dictionary."""
        return {
            "id": graph.id,
            "name": graph.name,
            "nodes": {
                node_id: {
                    "id": node.id,
                    "name": node.name,
                    "node_type": node.node_type,
                    "inputs": [
                        {
                            "id": pin.id,
                            "name": pin.name,
                            "pin_type": pin.pin_type.value,
                            "variable_type": pin.variable_type.value if pin.variable_type else None,
                            "is_connected": pin.is_connected,
                            "connected_to": pin.connected_to
                        }
                        for pin in node.inputs
                    ],
                    "outputs": [
                        {
                            "id": pin.id,
                            "name": pin.name,
                            "pin_type": pin.pin_type.value,
                            "variable_type": pin.variable_type.value if pin.variable_type else None,
                            "is_connected": pin.is_connected,
                            "connected_to": pin.connected_to
                        }
                        for pin in node.outputs
                    ],
                    "properties": node.properties,
                    "position": node.position
                }
                for node_id, node in graph.nodes.items()
            },
            "connections": graph.connections,
            "metadata": graph.metadata
        }
    
    @staticmethod
    def deserialize_graph(data: Dict[str, Any]):
        """Deserialize a CV3 graph from a dictionary."""
        from .graph import CV3Graph, CV3Node, CV3Pin, PinType, VariableType
        
        graph = CV3Graph(
            id=data["id"],
            name=data["name"],
            metadata=data.get("metadata", {})
        )
        
        # Create nodes
        nodes = {}
        for node_id, node_data in data["nodes"].items():
            node = CV3Node(
                id=node_data["id"],
                name=node_data["name"],
                node_type=node_data["node_type"],
                properties=node_data.get("properties", {}),
                position=tuple(node_data.get("position", [0.0, 0.0]))
            )
            
            # Recreate inputs
            for pin_data in node_data["inputs"]:
                pin = CV3Pin(
                    id=pin_data["id"],
                    name=pin_data["name"],
                    pin_type=PinType(pin_data["pin_type"]),
                    variable_type=VariableType(pin_data["variable_type"]) if pin_data["variable_type"] else None,
                    is_connected=pin_data["is_connected"],
                    connected_to=pin_data["connected_to"]
                )
                node.inputs.append(pin)
            
            # Recreate outputs
            for pin_data in node_data["outputs"]:
                pin = CV3Pin(
                    id=pin_data["id"],
                    name=pin_data["name"],
                    pin_type=PinType(pin_data["pin_type"]),
                    variable_type=VariableType(pin_data["variable_type"]) if pin_data["variable_type"] else None,
                    is_connected=pin_data["is_connected"],
                    connected_to=pin_data["connected_to"]
                )
                node.outputs.append(pin)
            
            nodes[node_id] = node
        
        graph.nodes = nodes
        
        # Recreate connections
        graph.connections = data.get("connections", [])
        
        # Note: We don't automatically restore pin connections here
        # In a full implementation, we would reconcile the connections
        # based on the connected_to fields in the pins
        
        return graph
    
    @staticmethod
    def serialize_state(state) -> Dict[str, Any]:
        """Serialize CV3 state to a dictionary."""
        return state.serialize()
    
    @staticmethod
    def deserialize_state(state, data: Dict[str, Any]):
        """Deserialize CV3 state from a dictionary."""
        state.deserialize(data)
    
    @staticmethod
    def serialize_tracer(tracer) -> Dict[str, Any]:
        """Serialize CV3 tracer to a dictionary."""
        return tracer.serialize()
    
    @staticmethod
    def deserialize_tracer(tracer, data: Dict[str, Any]):
        """Deserialize CV3 tracer from a dictionary."""
        tracer.deserialize(data)
    
    @staticmethod
    def serialize_quotas(quotas) -> Dict[str, Any]:
        """Serialize CV3 quotas to a dictionary."""
        return {
            quota_type.value: {
                "limit": quota.limit,
                "current_usage": quota.current_usage,
                "violated": quota.violated
            }
            for quota_type, quota in quotas.quotas.items()
        }
    
    @staticmethod
    def deserialize_quotas(quotas, data: Dict[str, Any]):
        """Deserialize CV3 quotas from a dictionary."""
        from .quotas import QuotaType, CV3Quota
        
        quotas.quotas = {}
        for quota_type_str, quota_data in data.items():
            quota_type = QuotaType(quota_type_str)
            quota = CV3Quota(quota_type, quota_data["limit"])
            quota.current_usage = quota_data["current_usage"]
            quota.violated = quota_data["violated"]
            quotas.quotas[quota_type] = quota
    
    @staticmethod
    def save_snapshot(graph, state, tracer=None, quotas=None) -> Dict[str, Any]:
        """Create a complete snapshot of CV3 execution state."""
        return {
            "timestamp": datetime.now().isoformat(),
            "version": "1.0",
            "graph": CV3Serializer.serialize_graph(graph),
            "state": CV3Serializer.serialize_state(state),
            "tracer": CV3Serializer.serialize_tracer(tracer) if tracer else None,
            "quotas": CV3Serializer.serialize_quotas(quotas) if quotas else None
        }
    
    @staticmethod
    def load_snapshot(snapshot: Dict[str, Any], graph, state, tracer=None, quotas=None):
        """Load a complete snapshot into CV3 objects."""
        # Load graph
        graph_data = snapshot["graph"]
        loaded_graph = CV3Serializer.deserialize_graph(graph_data)
        # Copy properties back to original graph
        graph.id = loaded_graph.id
        graph.name = loaded_graph.name
        graph.nodes = loaded_graph.nodes
        graph.connections = loaded_graph.connections
        graph.metadata = loaded_graph.metadata
        
        # Load state
        state_data = snapshot["state"]
        CV3Serializer.deserialize_state(state, state_data)
        
        # Load tracer if provided
        if tracer and snapshot.get("tracer"):
            tracer_data = snapshot["tracer"]
            CV3Serializer.deserialize_tracer(tracer, tracer_data)
        
        # Load quotas if provided
        if quotas and snapshot.get("quotas"):
            quotas_data = snapshot["quotas"]
            CV3Serializer.deserialize_quotas(quotas, quotas_data)
    
    @staticmethod
    def to_json(obj) -> str:
        """Serialize any CV3 object to JSON string."""
        return json.dumps(obj, indent=2, default=str)
    
    @staticmethod
    def from_json(json_str: str) -> Dict[str, Any]:
        """Deserialize JSON string to dictionary."""
        return json.loads(json_str)