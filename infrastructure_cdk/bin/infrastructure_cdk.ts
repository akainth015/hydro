#!/usr/bin/env node
import * as cdk from 'aws-cdk-lib/core';
import { InfrastructureStack } from '../lib/infrastructure_stack';

const app = new cdk.App();

new InfrastructureStack(app, 'HydroInfrastructureStack', {
  description: "Misc. testing infrastructure for Hydro programs",
  stackName: "HydroTestInfrastructure"
});
