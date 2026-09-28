#!/usr/bin/env python3
"""Synthetic off-chain lifecycle fixture. No network, trained model, signer, or paid compute."""
import hashlib,json
from pathlib import Path

def canonical(value):
    return json.dumps(value,sort_keys=True,separators=(',',':'))
def digest(value):
    return hashlib.sha256(value.encode()).hexdigest()
def generate():
    model={'name':'synthetic-moving-average','version':'fixture-v1','rule':'sum(samples) >= threshold * len(samples)','trained':False,'execution':'off-chain Python standard library'}
    inputs={'samples':[12,14,16],'threshold':13,'forecast_block':120,'unit':'synthetic units'}
    output=sum(inputs['samples']) >= inputs['threshold']*len(inputs['samples'])
    cases=[]
    for name,realized in [('correct',16),('incorrect',8),('good_faith_rejected',16),('prediction_fraud',16),('fraudulent_dispute',16),('no_winner',16)]:
        reported=(not output) if name=='prediction_fraud' else output
        prediction={'schema':'era-ai-fixture-v1','model_sha256':digest(canonical(model)),'model_version':model['version'],'inputs':inputs,'reported_output':reported,'confidence':75,'confidence_is_calibrated':False,'resolution_rule':'realized >= threshold','fixture_only':True}
        observation={'realized':realized,'threshold':inputs['threshold'],'actual_event':realized>=inputs['threshold'],'fixture_only':True}
        challenge=dict(observation)
        if name=='fraudulent_dispute': challenge.update(realized=8,actual_event=False)
        witness={'challenge_document_sha256':digest(canonical(challenge)),'reference_observation_sha256':digest(canonical(observation)),'challenge_matches_reference':challenge==observation,'prediction_sha256':digest(canonical(prediction)),'observation_sha256':digest(canonical(observation)),'recomputed_model_output':output,'reported_model_output':reported,'model_execution_claim_consistent':reported==output,'forecast_correct':reported==observation['actual_event'],'fraudulent_dispute_fixture':name=='fraudulent_dispute'}
        cases.append({'name':name,'prediction_json':canonical(prediction),'prediction_sha256':digest(canonical(prediction)),'observation_json':canonical(observation),'witness_json':canonical(witness),'evidence_sha256':digest(canonical(witness)),'model_execution_claim_consistent':reported==output,'forecast_correct':witness['forecast_correct'],'challenge_json':canonical(challenge),'challenge_matches_reference':challenge==observation})
    return {'fixture_only':True,'model_json':canonical(model),'model_sha256':digest(canonical(model)),'cases':cases}
if __name__=='__main__':
    path=Path(__file__).parent/'fixtures/lifecycle.json'
    data=generate();path.write_text(json.dumps(data,indent=2)+'\n')
    print(json.dumps({'fixture':str(path),'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'cases':len(data['cases']),'network':False,'trained_model':False}))
