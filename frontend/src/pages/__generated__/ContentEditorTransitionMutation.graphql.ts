/**
 * @generated SignedSource<<27fdf308e402e09b3312239d45a14d61>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type ContentEditorTransitionMutation$variables = {
  id: string;
  toState: string;
  type: string;
};
export type ContentEditorTransitionMutation$data = {
  readonly transitionContent: {
    readonly fields: any;
    readonly id: string;
    readonly status: string;
  };
};
export type ContentEditorTransitionMutation = {
  response: ContentEditorTransitionMutation$data;
  variables: ContentEditorTransitionMutation$variables;
};

const node: ConcreteRequest = (function(){
var v0 = {
  "defaultValue": null,
  "kind": "LocalArgument",
  "name": "id"
},
v1 = {
  "defaultValue": null,
  "kind": "LocalArgument",
  "name": "toState"
},
v2 = {
  "defaultValue": null,
  "kind": "LocalArgument",
  "name": "type"
},
v3 = [
  {
    "alias": null,
    "args": [
      {
        "kind": "Variable",
        "name": "id",
        "variableName": "id"
      },
      {
        "kind": "Variable",
        "name": "toState",
        "variableName": "toState"
      },
      {
        "kind": "Variable",
        "name": "type",
        "variableName": "type"
      }
    ],
    "concreteType": "ContentItem",
    "kind": "LinkedField",
    "name": "transitionContent",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "id",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "status",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "fields",
        "storageKey": null
      }
    ],
    "storageKey": null
  }
];
return {
  "fragment": {
    "argumentDefinitions": [
      (v0/*:: as any*/),
      (v1/*:: as any*/),
      (v2/*:: as any*/)
    ],
    "kind": "Fragment",
    "metadata": null,
    "name": "ContentEditorTransitionMutation",
    "selections": (v3/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [
      (v2/*:: as any*/),
      (v0/*:: as any*/),
      (v1/*:: as any*/)
    ],
    "kind": "Operation",
    "name": "ContentEditorTransitionMutation",
    "selections": (v3/*:: as any*/)
  },
  "params": {
    "cacheID": "bf2c5e4b9f83bfb19399f8612b34ff0c",
    "id": null,
    "metadata": {},
    "name": "ContentEditorTransitionMutation",
    "operationKind": "mutation",
    "text": "mutation ContentEditorTransitionMutation(\n  $type: String!\n  $id: ID!\n  $toState: String!\n) {\n  transitionContent(type: $type, id: $id, toState: $toState) {\n    id\n    status\n    fields\n  }\n}\n"
  }
};
})();

(node as any).hash = "46f7037ad06bf661697ffd94a26219aa";

export default node;
